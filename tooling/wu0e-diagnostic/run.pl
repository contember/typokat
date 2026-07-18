#!/usr/bin/env perl

use v5.20;
use strict;
use warnings;

use Cwd qw(abs_path);
use Digest::SHA qw(sha256_hex);
use Errno qw(EAGAIN EWOULDBLOCK);
use File::Basename qw(dirname);
use File::Path qw(remove_tree);
use File::Spec;
use Fcntl qw(:DEFAULT F_GETFL F_SETFL FD_CLOEXEC F_GETFD F_SETFD);
use IO::Handle;
use JSON::PP qw(decode_json);
use POSIX qw(WNOHANG setsid strftime);
use Time::HiRes qw(CLOCK_MONOTONIC clock_gettime usleep);

my $DEADLINE_US = 180_000_000;
my $MAX_PROCESS_GROUP_RSS_BYTES = 1024 * 1024 * 1024;
my $MAX_STDOUT_BYTES = 128 * 1024;
my $MAX_STDERR_BYTES = 128 * 1024;
my $MAX_TRACE_BYTES = 256 * 1024;
my $TERM_GRACE_US = 250_000;
my $DRAIN_GRACE_US = 250_000;
my $RSS_SAMPLE_TARGET_US = 1_000;
my $MAX_RSS_SAMPLE_INTERVAL_US = 10_000;
my $MAX_SAFE_INTEGER = 9_007_199_254_740_991;
my $PAGE_SIZE = POSIX::sysconf(POSIX::_SC_PAGESIZE());
my $CGROUP_ROOT = '/sys/fs/cgroup';
my $PRODUCTION_MEMORY_MAX = 1_073_741_824;
my $SELF_TEST_MEMORY_MAX = 64 * 1024 * 1024;
my $RSS_RETRY_ATTEMPTS = 3;
my $RSS_RETRY_DEADLINE_US = 10_000;
my $REEXEC_MARKER = 'TYPOKAT_WU0E_DELEGATED_REEXEC';
my $REEXEC_PARENT_CGROUP = 'TYPOKAT_WU0E_REEXEC_PARENT_CGROUP';

my $PROFILE_IDENTITY = 'ea59b3e150195f6cfe843661c0bcb006cffb04dd988861778a188be9441c579d';
my $PROFILE_MANIFEST_SHA256 = '1edef1b5e870024834762267ec532c3054f3b2279e9181844e21648243eb1407';
my $PROFILE_GITATTRIBUTES = "lib/*.d.ts -text -diff\n"
    . "LICENSE.txt -text -diff\n"
    . "ThirdPartyNoticeText.txt -text -diff\n"
    . "profile.toml text eol=lf\n"
    . "README.md text eol=lf\n"
    . "THIRD_PARTY_NOTICE.md text eol=lf\n";

my @MODES = qw(plain measured-off candidate-b);
my $WORKLOAD_PROBE =
    'check::checker::wu0e_diagnostic::wu0e_primary_probe_once';
my $VALIDATOR_PROBE =
    'check::checker::wu0e_diagnostic::wu0e_validate_trace_once';

sub usage {
    return <<'USAGE';
Usage:
  perl tooling/wu0e-diagnostic/run.pl
  perl tooling/wu0e-diagnostic/run.pl --dry-run
  perl tooling/wu0e-diagnostic/run.pl --self-test

The diagnostic run executes plain, measured-off, and candidate-b in that order.
USAGE
}

sub fail {
    die "wu0e-diagnostic: $_[0]\n";
}

sub shell_quote {
    my ($value) = @_;
    return "'" . ($value =~ s/'/'"'"'/gr) . "'";
}

sub command_text {
    return join ' ', map { shell_quote($_) } @_;
}

sub decode_wait_status {
    my ($status) = @_;
    return 128 + ($status & 127) if $status & 127;
    return ($status >> 8) & 255;
}

sub read_bytes {
    my ($path) = @_;
    open my $handle, '<:raw', $path or fail("cannot open $path: $!");
    local $/;
    my $bytes = <$handle>;
    defined $bytes or $bytes = '';
    close $handle or fail("cannot close $path: $!");
    return $bytes;
}

sub read_bounded_file {
    my ($path, $limit, $post_open_hook) = @_;
    return ('', 0, 0) unless -e $path;
    my @before = lstat $path;
    @before && -f _ && !-l _
        or fail("bounded input is not a regular non-symlink file: $path");
    sysopen my $handle, $path, O_RDONLY | O_NOFOLLOW
        or fail("cannot open bounded input $path: $!");
    binmode $handle, ':raw';
    $post_open_hook->() if $post_open_hook;
    my $bytes = '';
    while (length($bytes) <= $limit) {
        my $remaining = $limit + 1 - length($bytes);
        my $chunk = '';
        my $count = sysread($handle, $chunk, $remaining);
        defined $count or fail("cannot read bounded input $path: $!");
        last if $count == 0;
        $bytes .= $chunk;
    }
    my @opened = stat $handle;
    close $handle or fail("cannot close bounded input $path: $!");
    my @after = lstat $path;
    @opened && @after && -f _ && !-l _
        or fail("bounded input changed type or disappeared: $path");
    $before[0] == $opened[0] && $before[1] == $opened[1]
        && $opened[0] == $after[0] && $opened[1] == $after[1]
        or fail('artifact inode changed during bounded access');
    $before[7] == $opened[7] && $opened[7] == $after[7]
        or fail('artifact size changed during bounded access');
    my $read_bytes = length($bytes);
    return ('', 1, $read_bytes) if $read_bytes > $limit;
    return ($bytes, 0, $read_bytes);
}

sub write_bytes_exclusive {
    my ($path, $bytes) = @_;
    sysopen my $handle, $path, O_WRONLY | O_CREAT | O_EXCL | O_NOFOLLOW, 0600
        or fail("cannot create $path: $!");
    binmode $handle, ':raw';
    print {$handle} $bytes or fail("cannot write $path: $!");
    close $handle or fail("cannot close $path: $!");
}

sub file_size {
    my ($path, $may_be_missing) = @_;
    my @stat = lstat $path;
    return 0 if !@stat && $may_be_missing;
    @stat or fail("artifact disappeared: $path");
    -f _ && !-l _ or fail("artifact is not a regular non-symlink file: $path");
    return $stat[7];
}

sub create_capture {
    my ($path) = @_;
    sysopen my $handle, $path, O_WRONLY | O_CREAT | O_EXCL | O_NOFOLLOW, 0600
        or fail("cannot create capture $path: $!");
    binmode $handle, ':raw';
    return $handle;
}

sub trim_one_line {
    my ($bytes, $label) = @_;
    $bytes =~ /\A([^\r\n]*)\n?\z/
        or fail("$label is not one canonical line");
    return $1;
}

sub read_control_file {
    my ($path, $label) = @_;
    my @stat = lstat $path;
    @stat && -f _ && !-l _ or fail("$label is unavailable: $path");
    sysopen my $handle, $path, O_RDONLY | O_NOFOLLOW
        or fail("cannot open $label $path: $!");
    my $bytes = '';
    my $count = sysread($handle, $bytes, 65_537);
    defined $count && $count <= 65_536
        or fail("cannot read bounded $label $path: $!");
    my $extra = '';
    my $extra_count = sysread($handle, $extra, 1);
    defined $extra_count && $extra_count == 0
        or fail("$label exceeds its read bound: $path");
    close $handle or fail("cannot close $label $path: $!");
    return $bytes;
}

sub write_control_file {
    my ($path, $bytes, $label) = @_;
    my @stat = lstat $path;
    @stat && -f _ && !-l _ or fail("$label is unavailable: $path");
    sysopen my $handle, $path, O_WRONLY | O_NOFOLLOW
        or fail("cannot open writable $label $path: $!");
    my $written = syswrite($handle, $bytes);
    defined $written && $written == length($bytes)
        or fail("cannot write $label $path: $!");
    close $handle or fail("cannot close $label $path: $!");
}

sub canonical_cgroup_path {
    my ($path, $label) = @_;
    defined $path && $path =~ m{\A/(?:[A-Za-z0-9_.:@-]+/)*[A-Za-z0-9_.:@-]+\z}
        && $path !~ m{(?:\A|/)\.\.?(?:/|\z)}
        or fail("unsafe $label control group");
    return $path;
}

sub self_control_group {
    return process_control_group('self');
}

sub process_control_group {
    my ($pid) = @_;
    $pid eq 'self' || $pid =~ /\A[1-9][0-9]*\z/
        or fail('unsafe process cgroup identity');
    my $bytes = read_control_file(
        "/proc/$pid/cgroup", 'process cgroup identity');
    my @lines = grep { $_ ne '' } split /\n/, $bytes;
    @lines == 1 && $lines[0] =~ /\A0::(.+)\z/
        or fail('unified cgroup-v2 identity is unavailable');
    return canonical_cgroup_path($1, 'self');
}

sub cgroup_unit_name {
    my ($control_group) = @_;
    my ($unit) = $control_group =~ m{/([A-Za-z0-9_.:@-]+\.scope)\z};
    defined $unit or fail('delegated control group is not a systemd scope');
    return $unit;
}

sub systemctl_property {
    my ($unit, $property) = @_;
    $unit =~ /\A[A-Za-z0-9_.:@-]+\.scope\z/
        or fail('unsafe delegated scope unit');
    $property =~ /\A(?:ControlGroup|Delegate)\z/
        or fail('unsafe systemctl property');
    my $value = capture_command_line(
        '/usr/bin/systemctl', '--user', 'show', $unit,
        "--property=$property", '--value');
    return trim_one_line("$value\n", "systemctl $property");
}

sub cgroup_members {
    my ($path) = @_;
    my $bytes = read_control_file(
        File::Spec->catfile($path, 'cgroup.procs'), 'cgroup.procs');
    my @members;
    for my $line (grep { $_ ne '' } split /\n/, $bytes) {
        $line =~ /\A[1-9][0-9]*\z/ or fail('malformed cgroup.procs member');
        push @members, 0 + $line;
    }
    return @members;
}

sub cgroup_tokens {
    my ($path, $name) = @_;
    my $line = trim_one_line(
        read_control_file(File::Spec->catfile($path, $name), $name), $name);
    my @tokens = grep { $_ ne '' } split / /, $line;
    for my $token (@tokens) {
        $token =~ /\A[A-Za-z0-9_-]+\z/ or fail("malformed $name token");
    }
    return @tokens;
}

sub assert_real_directory_path {
    my ($path, $label) = @_;
    my @stat = lstat $path;
    @stat && -d _ && !-l _ or fail("$label is not a real directory: $path");
}

sub delegated_scope_identity {
    my $control_group = self_control_group();
    my $unit = cgroup_unit_name($control_group);
    my $systemctl_control_group = systemctl_property($unit, 'ControlGroup');
    my $delegate = systemctl_property($unit, 'Delegate');
    canonical_cgroup_path($systemctl_control_group, 'systemctl');
    $systemctl_control_group eq $control_group
        or fail('delegated scope ControlGroup mismatch');
    $delegate eq 'yes' or fail('delegated scope Delegate is not yes');
    my @components = split m{/}, substr($control_group, 1);
    my $path = File::Spec->catdir($CGROUP_ROOT, @components);
    assert_real_directory_path($path, 'delegated cgroup root');
    return {
        unit => $unit,
        control_group => $control_group,
        path => $path,
        delegate => $delegate,
    };
}

sub request_verified_scope_abort {
    my (%args) = @_;
    my $scope = $args{scope};
    systemctl_property($scope->{unit}, 'ControlGroup') eq $scope->{control_group}
        or fail('scope abort ControlGroup cross-check failed');
    systemctl_property($scope->{unit}, 'Delegate') eq 'yes'
        or fail('scope abort Delegate cross-check failed');
    $args{identity_verified_callback}->(
        $scope->{unit}, $scope->{control_group})
        if defined $args{identity_verified_callback};
    my @command = (
        '/usr/bin/systemctl', '--user', '--no-block', 'stop', $scope->{unit});
    if (defined $args{injected_request_callback}) {
        my $callback_count = 0;
        ++$callback_count;
        my $outcome = $args{injected_request_callback}->(
            $scope->{unit}, $scope->{control_group}, \@command);
        ref($outcome) eq 'HASH'
            && ($outcome->{abort_request_observed} // 0) == 1
            && ($outcome->{retained_launch_removed} // 0) == 1
            or fail('injected scope abort request returned an invalid outcome');
        $args{request_observed_callback}->(\@command)
            if defined $args{request_observed_callback};
        return {
            %$outcome,
            abort_request_callback_count => $callback_count,
            systemctl_argv => join('|', @command),
        };
    }
    exec { $command[0] } @command
        or fail("cannot request delegated scope abort: $!");
}

sub record_reexec_argv {
    my ($script, $arguments) = @_;
    return unless @$arguments == 4 && $arguments->[0] eq '--self-test-evidence';
    my $evidence = $arguments->[1];
    assert_real_directory_path($evidence, 'hardening evidence directory');
    my @command = (
        '/usr/bin/systemd-run', '--user', '--scope', '--quiet',
        '--no-ask-password', '--property=Delegate=yes',
        '--expand-environment=no', '--', '/usr/bin/perl', $script,
        @$arguments,
    );
    write_bytes_exclusive(
        File::Spec->catfile($evidence, 'reexec-argv.txt'),
        join('', map { "$_\n" } @command));
    write_bytes_exclusive(
        File::Spec->catfile($evidence, 'systemd-run-count'), "1\n");
}

sub ensure_delegated_scope {
    my (@arguments) = @_;
    my $script = abs_path($0) // fail("cannot resolve script path $0");
    my $marker = $ENV{$REEXEC_MARKER};
    if (!defined $marker) {
        my $parent_cgroup = self_control_group();
        my $parent_stat = hardened_linux_process_stat($$)
            // fail('cannot inspect pre-reexec coordinator identity');
        my $boot_id = trim_one_line(
            read_control_file('/proc/sys/kernel/random/boot_id', 'kernel boot identity'),
            'kernel boot identity');
        my $token = sha256_hex(join "\0", $$, $parent_stat->{start_ticks},
            $script, $parent_cgroup, $boot_id);
        $ENV{$REEXEC_MARKER} =
            "pending:$$:$parent_stat->{start_ticks}:$token";
        $ENV{$REEXEC_PARENT_CGROUP} = $parent_cgroup;
        record_reexec_argv($script, \@arguments);
        my @command = (
            '/usr/bin/systemd-run', '--user', '--scope', '--quiet',
            '--no-ask-password', '--property=Delegate=yes',
            '--expand-environment=no', '--', '/usr/bin/perl', $script,
            @arguments,
        );
        exec { $command[0] } @command
            or fail("cannot reexec through /usr/bin/systemd-run: $!");
    }
    $marker =~ /\Aactive:[0-9a-f]{64}\z/
        and fail('nested delegated-scope reexec');
    my ($reexec_pid, $reexec_start_ticks, $token) = $marker =~
        /\Apending:([1-9][0-9]*):([1-9][0-9]*):([0-9a-f]{64})\z/;
    defined $reexec_pid && defined $reexec_start_ticks && defined $token
        or fail('forged delegated-scope marker');
    defined $ENV{$REEXEC_PARENT_CGROUP}
        or fail('forged delegated-scope marker');
    my $parent_cgroup = eval {
        canonical_cgroup_path($ENV{$REEXEC_PARENT_CGROUP}, 'reexec parent');
    };
    defined $parent_cgroup or fail('forged delegated-scope marker');
    $$ == $reexec_pid or fail('forged delegated-scope marker');
    my $reexec_stat = hardened_linux_process_stat($$)
        // fail('forged delegated-scope marker');
    $reexec_stat->{start_ticks} == $reexec_start_ticks
        or fail('forged delegated-scope marker');
    my $boot_id = trim_one_line(
        read_control_file('/proc/sys/kernel/random/boot_id', 'kernel boot identity'),
        'kernel boot identity');
    my $expected_token = sha256_hex(join "\0", $reexec_pid,
        $reexec_start_ticks, $script, $parent_cgroup, $boot_id);
    $token eq $expected_token or fail('forged delegated-scope marker');
    my $scope = eval { delegated_scope_identity() };
    defined $scope or fail('forged delegated-scope marker');
    $scope->{control_group} ne $parent_cgroup
        or fail('forged delegated-scope marker');
    $ENV{$REEXEC_MARKER} = "active:$token";
    return $scope;
}

sub setup_delegated_root {
    my ($scope) = @_;
    my $root = $scope->{path};
    trim_one_line(
        read_control_file(File::Spec->catfile($root, 'cgroup.type'), 'cgroup.type'),
        'cgroup.type') eq 'domain'
        or fail('delegated cgroup root is not a domain');
    my @available = cgroup_tokens($root, 'cgroup.controllers');
    scalar(grep { $_ eq 'memory' } @available) == 1
        or fail('delegated cgroup has no memory controller');
    my @before = sort(cgroup_tokens($root, 'cgroup.subtree_control'));
    my $supervisor = File::Spec->catdir($root, 'supervisor');
    my $memory_was_enabled = scalar(grep { $_ eq 'memory' } @before) == 1;
    my $enabled_by_runner = $memory_was_enabled ? 0 : 1;
    my ($created, $moved, $result) = (0, 0, undef);
    my $ok = eval {
        mkdir $supervisor, 0700
            or fail("cannot create supervisor cgroup $supervisor: $!");
        $created = 1;
        write_control_file(
            File::Spec->catfile($supervisor, 'cgroup.procs'), "$$\n",
            'supervisor membership');
        $moved = 1;
        my @supervisor_members = cgroup_members($supervisor);
        @supervisor_members == 1 && $supervisor_members[0] == $$
            or fail('coordinator did not enter supervisor cgroup');
        my @root_members = cgroup_members($root);
        @root_members == 0 or fail('delegated root still has internal processes');
        if ($enabled_by_runner) {
            write_control_file(
                File::Spec->catfile($root, 'cgroup.subtree_control'), "+memory\n",
                'memory controller enable');
        }
        my @enabled = cgroup_tokens($root, 'cgroup.subtree_control');
        scalar(grep { $_ eq 'memory' } @enabled) == 1
            or fail('memory controller did not enable');
        $result = {
            %$scope,
            supervisor => $supervisor,
            controllers_before => join(',', @before) || 'none',
            enabled_by_runner => $enabled_by_runner,
            memory_controller_available => 1,
        };
        1;
    };
    if (!$ok) {
        my $primary_error = $@;
        my $cleanup_ok = eval {
            if ($enabled_by_runner) {
                my @current = cgroup_tokens($root, 'cgroup.subtree_control');
                if (scalar(grep { $_ eq 'memory' } @current) == 1) {
                    write_control_file(
                        File::Spec->catfile($root, 'cgroup.subtree_control'),
                        "-memory\n", 'failed-setup memory disable');
                }
            }
            if ($moved) {
                write_control_file(
                    File::Spec->catfile($root, 'cgroup.procs'), "$$\n",
                    'failed-setup root membership');
            }
            if ($created && -d $supervisor) {
                my @members = cgroup_members($supervisor);
                @members == 0
                    or fail('failed-setup supervisor remains populated');
                rmdir $supervisor
                    or fail("cannot remove failed-setup supervisor: $!");
            }
            1;
        };
        $primary_error .= $@ unless $cleanup_ok;
        die $primary_error;
    }
    return $result;
}

sub teardown_delegated_root {
    my ($scope) = @_;
    my $root = $scope->{path};
    if ($scope->{enabled_by_runner}) {
        write_control_file(
            File::Spec->catfile($root, 'cgroup.subtree_control'), "-memory\n",
            'memory controller disable');
    }
    write_control_file(
        File::Spec->catfile($root, 'cgroup.procs'), "$$\n",
        'delegated-root membership');
    my @supervisor_members = cgroup_members($scope->{supervisor});
    @supervisor_members == 0 or fail('supervisor cgroup is not empty at teardown');
    rmdir $scope->{supervisor}
        or fail("cannot remove supervisor cgroup $scope->{supervisor}: $!");
    my @after = sort(cgroup_tokens($root, 'cgroup.subtree_control'));
    my $after = join(',', @after) || 'none';
    $after eq $scope->{controllers_before}
        or fail('delegated controller state was not restored');
    return $after;
}

sub numeric_control_value {
    my ($path, $label) = @_;
    my $value = trim_one_line(read_control_file($path, $label), $label);
    $value =~ /\A(?:0|[1-9][0-9]*)\z/
        or fail("malformed numeric $label");
    $value <= $MAX_SAFE_INTEGER or fail("numeric $label exceeds exact range");
    return 0 + $value;
}

sub cgroup_events {
    my ($launch_path) = @_;
    my $bytes = read_control_file(
        File::Spec->catfile($launch_path, 'cgroup.events'), 'cgroup.events');
    my %events;
    for my $line (grep { $_ ne '' } split /\n/, $bytes) {
        my ($key, $value) = $line =~ /\A([a-z_]+) (0|[1-9][0-9]*)\z/;
        defined $key && $value <= $MAX_SAFE_INTEGER
            or fail('malformed cgroup.events');
        exists $events{$key} and fail("duplicate cgroup.events field $key");
        $events{$key} = 0 + $value;
    }
    exists $events{populated} && ($events{populated} == 0 || $events{populated} == 1)
        or fail('cgroup.events has no canonical populated field');
    return \%events;
}

sub memory_events_local {
    my ($launch_path) = @_;
    my $bytes = read_control_file(
        File::Spec->catfile($launch_path, 'memory.events.local'),
        'memory.events.local');
    my %events;
    for my $line (grep { $_ ne '' } split /\n/, $bytes) {
        my ($key, $value) = $line =~ /\A([a-z_]+) (0|[1-9][0-9]*)\z/;
        defined $key && $value <= $MAX_SAFE_INTEGER
            or fail('malformed memory.events.local');
        exists $events{$key}
            and fail("duplicate memory.events.local field $key");
        $events{$key} = 0 + $value;
    }
    for my $required (qw(max oom oom_kill oom_group_kill)) {
        exists $events{$required}
            or fail("memory.events.local lacks $required");
    }
    return \%events;
}

sub open_writable_without_write {
    my ($path, $label) = @_;
    my @stat = lstat $path;
    @stat && -f _ && !-l _ or fail("$label is unavailable: $path");
    sysopen my $handle, $path, O_WRONLY | O_NOFOLLOW
        or fail("$label is not writable: $path: $!");
    close $handle or fail("cannot close writable $label $path: $!");
}

sub preflight_policy_failure {
    my ($view) = @_;
    return 'cgroup-unavailable' unless $view->{cgroup_available};
    return 'delegate-false'
        unless defined $view->{delegate} && $view->{delegate} eq 'yes';
    return 'memory-controller-missing'
        unless $view->{memory_controller_available};
    return 'cgroup-type-missing' unless $view->{cgroup_type_available};
    return 'cgroup-type-threaded'
        unless defined $view->{cgroup_type} && $view->{cgroup_type} eq 'domain';
    return 'cgroup-procs-inaccessible' unless $view->{cgroup_procs_accessible};
    return 'cgroup-events-malformed' unless $view->{cgroup_events_valid};
    return 'cgroup-kill-unwritable' unless $view->{cgroup_kill_writable};
    return 'memory-max-readback-mismatch'
        unless defined $view->{memory_max_readback}
            && defined $view->{memory_max_requested}
            && $view->{memory_max_readback} == $view->{memory_max_requested};
    return 'memory-swap-max-readback-mismatch'
        unless defined $view->{memory_swap_max_readback}
            && $view->{memory_swap_max_readback} == 0;
    return 'memory-oom-group-readback-mismatch'
        unless defined $view->{memory_oom_group_readback}
            && $view->{memory_oom_group_readback} == 1;
    return 'memory-current-missing' unless $view->{memory_current_available};
    return 'memory-peak-malformed' unless $view->{memory_peak_valid};
    return 'memory-events-local-unreadable'
        unless $view->{memory_events_local_readable};
    return;
}

sub evaluate_preflight_admission {
    my (%args) = @_;
    my $failure = preflight_policy_failure($args{view});
    return { termination => 'infrastructure', failure => $failure }
        if defined $failure;
    $args{workload_callback}->() if defined $args{workload_callback};
    $args{validator_callback}->() if defined $args{validator_callback};
    return { termination => 'admitted', failure => 'none' };
}

sub configure_launch_cgroup {
    my (%args) = @_;
    my $name = $args{name};
    $name =~ /\A[A-Za-z0-9_.-]+\z/ or fail('unsafe launch cgroup name');
    my $path = File::Spec->catdir($args{scope}{path}, $name);
    mkdir $path, 0700 or fail("cannot create launch cgroup $path: $!");
    my $configured;
    my $ok = eval {
        my @checked_files;
        my $type = trim_one_line(
            read_control_file(File::Spec->catfile($path, 'cgroup.type'), 'cgroup.type'),
            'cgroup.type');
        push @checked_files, 'cgroup.type';
        $type eq 'domain' or fail('launch cgroup.type is not domain');
        cgroup_members($path);
        open_writable_without_write(
            File::Spec->catfile($path, 'cgroup.procs'), 'cgroup.procs');
        push @checked_files, 'cgroup.procs';
        cgroup_events($path);
        push @checked_files, 'cgroup.events';
        open_writable_without_write(
            File::Spec->catfile($path, 'cgroup.kill'), 'cgroup.kill');
        push @checked_files, 'cgroup.kill';
        for my $file (qw(memory.max memory.swap.max memory.oom.group)) {
            read_control_file(File::Spec->catfile($path, $file), $file);
            push @checked_files, $file;
        }
        numeric_control_value(
            File::Spec->catfile($path, 'memory.current'), 'memory.current');
        push @checked_files, 'memory.current';
        numeric_control_value(
            File::Spec->catfile($path, 'memory.peak'), 'memory.peak');
        push @checked_files, 'memory.peak';
        memory_events_local($path);
        push @checked_files, 'memory.events.local';
        my $memory_max = $args{memory_max} // $PRODUCTION_MEMORY_MAX;
        $memory_max =~ /\A(?:0|[1-9][0-9]*)\z/
            or fail('invalid memory.max request');
        write_control_file(
            File::Spec->catfile($path, 'memory.max'), "$memory_max\n",
            'memory.max');
        write_control_file(
            File::Spec->catfile($path, 'memory.swap.max'), "0\n",
            'memory.swap.max');
        write_control_file(
            File::Spec->catfile($path, 'memory.oom.group'), "1\n",
            'memory.oom.group');
        my $memory_max_readback = numeric_control_value(
            File::Spec->catfile($path, 'memory.max'), 'memory.max');
        my $memory_swap_readback = numeric_control_value(
            File::Spec->catfile($path, 'memory.swap.max'), 'memory.swap.max');
        my $memory_oom_group_readback = numeric_control_value(
            File::Spec->catfile($path, 'memory.oom.group'), 'memory.oom.group');
        $memory_max_readback == $memory_max
            or fail('memory.max readback mismatch');
        $memory_swap_readback == 0 or fail('memory.swap.max readback mismatch');
        $memory_oom_group_readback == 1
            or fail('memory.oom.group readback mismatch');
        my $memory_current = numeric_control_value(
            File::Spec->catfile($path, 'memory.current'), 'memory.current');
        my $memory_peak = numeric_control_value(
            File::Spec->catfile($path, 'memory.peak'), 'memory.peak');
        my $baseline = memory_events_local($path);
        my $admission = evaluate_preflight_admission(view => {
            cgroup_available => -d $path ? 1 : 0,
            delegate => $args{scope}{delegate},
            memory_controller_available =>
                $args{scope}{memory_controller_available} // 0,
            cgroup_type_available => 1,
            cgroup_type => $type,
            cgroup_procs_accessible => 1,
            cgroup_events_valid => 1,
            cgroup_kill_writable => 1,
            memory_max_requested => 0 + $memory_max,
            memory_max_readback => $memory_max_readback,
            memory_swap_max_readback => $memory_swap_readback,
            memory_oom_group_readback => $memory_oom_group_readback,
            memory_current_available => 1,
            memory_peak_valid => 1,
            memory_events_local_readable => 1,
        });
        $admission->{termination} eq 'admitted'
            or fail("launch preflight policy rejected: $admission->{failure}");
        $configured = {
            path => $path,
            cgroup_type => 'domain',
            memory_max => $memory_max_readback,
            memory_swap_max => $memory_swap_readback,
            memory_oom_group => $memory_oom_group_readback,
            memory_current_preflight => $memory_current,
            memory_peak_preflight => $memory_peak,
            events_baseline => $baseline,
            checked_files => \@checked_files,
            cgroup_kill_access => 'writable',
        };
        1;
    };
    if (!$ok) {
        my $error = $@;
        rmdir $path
            or $error .= "wu0e-diagnostic: cannot remove failed launch cgroup $path: $!\n";
        die $error;
    }
    return $configured;
}

sub linux_state_is_live {
    my ($state) = @_;
    defined $state && $state =~ /\A[A-Za-z]\z/
        or fail('malformed Linux process state');
    return $state !~ /\A[ZXxz]\z/;
}

sub hardened_linux_process_stat {
    my ($pid) = @_;
    $pid =~ /\A[1-9][0-9]*\z/ or return;
    my $path = "/proc/$pid/stat";
    sysopen my $handle, $path, O_RDONLY or return;
    my $bytes = '';
    my $count = sysread($handle, $bytes, 4097);
    close $handle;
    return unless defined $count && $count > 0 && $count <= 4096;
    my ($actual_pid, $state, $tail) =
        $bytes =~ /\A([0-9]+) \(.*\) ([A-Za-z]) (.+)\n?\z/s;
    return unless defined $actual_pid && $actual_pid == $pid;
    my @fields = split / /, $tail;
    return unless @fields >= 19
        && $fields[0] =~ /\A[0-9]+\z/
        && $fields[1] =~ /\A[0-9]+\z/
        && $fields[18] =~ /\A[0-9]+\z/;
    return {
        state => $state,
        parent => 0 + $fields[0],
        pgrp => 0 + $fields[1],
        start_ticks => 0 + $fields[18],
    };
}

sub cgroup_member_snapshot {
    my ($launch_path) = @_;
    my @members = sort { $a <=> $b } cgroup_members($launch_path);
    my %seen;
    scalar(grep { !$seen{$_}++ } @members) == scalar(@members)
        or fail('duplicate cgroup.procs member');
    return \@members;
}

sub process_rss_from_identity {
    my ($pid, $expected_start) = @_;
    my $before = hardened_linux_process_stat($pid);
    return { vanished => 1 } unless defined $before;
    return { dead => 1 } unless linux_state_is_live($before->{state});
    $before->{start_ticks} == $expected_start
        or return { vanished => 1 };
    my $path = "/proc/$pid/statm";
    sysopen my $handle, $path, O_RDONLY or return { unreadable => 1 };
    my $bytes = '';
    my $count = sysread($handle, $bytes, 257);
    close $handle;
    return { unreadable => 1 }
        unless defined $count && $count > 0 && $count <= 256;
    my ($resident_pages) = $bytes =~ /\A[0-9]+ ([0-9]+)(?: [0-9]+)*\n?\z/;
    return { unreadable => 1 } unless defined $resident_pages;
    my $rss = checked_mul(0 + $resident_pages, $PAGE_SIZE);
    return { arithmetic => 1 } unless defined $rss;
    my $after = hardened_linux_process_stat($pid);
    return { vanished => 1 } unless defined $after
        && $after->{start_ticks} == $expected_start;
    return { dead => 1 } unless linux_state_is_live($after->{state});
    return { rss => $rss };
}

sub sample_cgroup_rss_attempt {
    my ($launch_path) = @_;
    my $last_problem = 'unresolved cgroup membership churn';
    my $members = cgroup_member_snapshot($launch_path);
    my ($sum, $largest) = (0, 0);
    my $retry = 0;
    for my $pid (@$members) {
        my $stat = hardened_linux_process_stat($pid);
        if (!defined $stat) {
            $retry = 1;
            $last_problem = "vanished cgroup member $pid";
            last;
        }
        next unless linux_state_is_live($stat->{state});
        my $sample = process_rss_from_identity($pid, $stat->{start_ticks});
        if ($sample->{arithmetic}) {
            return {
                status => 'infrastructure',
                problem => 'RSS arithmetic uncertainty',
            };
        }
        if ($sample->{vanished} || $sample->{unreadable}) {
            $retry = 1;
            $last_problem = $sample->{unreadable}
                ? "stably unreadable cgroup member $pid"
                : "vanished cgroup member $pid";
            last;
        }
        next if $sample->{dead};
        $sum = checked_add($sum, $sample->{rss});
        defined $sum or return {
            status => 'infrastructure',
            problem => 'RSS group sum uncertainty',
        };
        $largest = $sample->{rss} if $sample->{rss} > $largest;
    }
    my $after = cgroup_member_snapshot($launch_path);
    if (join(',', @$members) ne join(',', @$after)) {
        $retry = 1;
        $last_problem = 'unresolved cgroup membership churn';
    }
    return { status => 'retry', problem => $last_problem } if $retry;
    return {
        status => 'complete', sum => $sum, largest => $largest,
        members => scalar(@$members), problem => 'none',
    };
}

sub execute_rss_retry_policy {
    my (%args) = @_;
    my $attempt_callback = $args{attempt_callback};
    defined $attempt_callback or fail('RSS retry policy lacks attempt callback');
    my $clock_callback = $args{clock_callback}
        // sub { clock_gettime(CLOCK_MONOTONIC) };
    my $sleep_callback = $args{sleep_callback} // sub { usleep(250) };
    my $started = $clock_callback->();
    my @journal;
    for my $attempt (1 .. $RSS_RETRY_ATTEMPTS) {
        my $sample = $attempt_callback->($attempt);
        ref($sample) eq 'HASH'
            && defined $sample->{status}
            && $sample->{status} =~ /\A(?:complete|retry|infrastructure)\z/
            or fail('RSS retry attempt returned an invalid decision');
        if ($sample->{status} eq 'complete') {
            push @journal, { attempt => $attempt, result => 'complete' };
            return { %$sample, journal => \@journal };
        }
        if ($sample->{status} eq 'infrastructure') {
            push @journal, { attempt => $attempt, result => 'infrastructure' };
            return { %$sample, journal => \@journal };
        }
        my $deadline_hit =
            ($clock_callback->() - $started) * 1_000_000
                >= $RSS_RETRY_DEADLINE_US;
        if ($attempt == $RSS_RETRY_ATTEMPTS || $deadline_hit) {
            push @journal, { attempt => $attempt, result => 'infrastructure' };
            return {
                %$sample, status => 'infrastructure', journal => \@journal,
            };
        }
        push @journal, { attempt => $attempt, result => 'retry' };
        $sleep_callback->();
    }
    fail('RSS retry policy exhausted without a terminal result');
}

sub sample_cgroup_rss {
    my ($launch_path) = @_;
    my $sample = execute_rss_retry_policy(
        attempt_callback => sub { sample_cgroup_rss_attempt($launch_path) });
    return ($sample->{sum}, $sample->{largest}, $sample->{members}, undef)
        if $sample->{status} eq 'complete';
    return (undef, undef, undef, $sample->{problem});
}

sub digest_open_handle {
    my ($handle) = @_;
    seek($handle, 0, 0) or fail("cannot seek executable handle: $!");
    my $digest = Digest::SHA->new(256);
    $digest->addfile($handle);
    seek($handle, 0, 0) or fail("cannot rewind executable handle: $!");
    return $digest->hexdigest;
}

sub open_stable_executable {
    my ($path, $expected_sha) = @_;
    my @before = lstat $path;
    @before && -f _ && -x _ && !-l _
        or fail("frozen executable is unsafe: $path");
    sysopen my $handle, $path, O_RDONLY | O_NOFOLLOW
        or fail("cannot open frozen executable $path: $!");
    binmode $handle, ':raw';
    my @opened = stat $handle;
    @opened && -f _ && -x _
        or fail("opened frozen executable is unsafe: $path");
    ($before[0] == $opened[0] && $before[1] == $opened[1])
        or fail('frozen executable pathname identity drifted');
    digest_open_handle($handle) eq $expected_sha
        or fail("frozen executable digest mismatch: $path");
    my $flags = fcntl($handle, F_GETFD, 0);
    defined $flags or fail("cannot inspect frozen executable descriptor: $!");
    fcntl($handle, F_SETFD, $flags & ~FD_CLOEXEC)
        or fail("cannot preserve frozen executable descriptor: $!");
    return ($handle, {
        device => $opened[0], inode => $opened[1], size => $opened[7],
        digest => $expected_sha,
    });
}

sub verify_stable_executable_path {
    my ($path, $identity) = @_;
    my @stat = lstat $path;
    @stat && -f _ && -x _ && !-l _
        && $stat[0] == $identity->{device}
        && $stat[1] == $identity->{inode}
        && $stat[7] == $identity->{size}
        or fail('frozen executable pathname identity drifted');
}

sub adjudicate_termination {
    my ($flags, $exit_code) = @_;
    return 'infrastructure' if $flags->{infrastructure};
    return 'trace' if $flags->{trace};
    return 'stdout' if $flags->{stdout};
    return 'stderr' if $flags->{stderr};
    return 'rss' if $flags->{rss};
    return 'deadline' if $flags->{deadline};
    return 'crash' if $flags->{crash} || !defined $exit_code || $exit_code != 0;
    return 'normal';
}

sub memory_event_delta {
    my ($baseline, $final, $key) = @_;
    exists $baseline->{$key} && exists $final->{$key}
        or fail("missing memory event $key");
    $final->{$key} >= $baseline->{$key}
        or fail("memory event $key decreased");
    return $final->{$key} - $baseline->{$key};
}

sub memory_source {
    my ($deltas) = @_;
    return 'oom_group_kill' if $deltas->{oom_group_kill} > 0;
    return 'oom_kill' if $deltas->{oom_kill} > 0;
    return 'oom' if $deltas->{oom} > 0;
    return 'max' if $deltas->{max} > 0;
    return 'none';
}

sub capture_final_cgroup_memory {
    my ($launch, $observed) = @_;
    my $final_events = memory_events_local($launch->{path});
    my %deltas;
    for my $family (qw(max oom oom_kill oom_group_kill)) {
        $deltas{$family} = memory_event_delta(
            $launch->{events_baseline}, $final_events, $family);
        $observed->{"events_${family}_baseline"} =
            $launch->{events_baseline}{$family};
        $observed->{"events_${family}_final"} = $final_events->{$family};
        $observed->{"events_${family}_delta"} = $deltas{$family};
    }
    $observed->{memory_current} = numeric_control_value(
        File::Spec->catfile($launch->{path}, 'memory.current'), 'memory.current');
    $observed->{memory_peak} = numeric_control_value(
        File::Spec->catfile($launch->{path}, 'memory.peak'), 'memory.peak');
    $observed->{memory_source} = memory_source(\%deltas);
}

sub pgid_has_live_cgroup_member {
    my ($launch_path, $pgid) = @_;
    for my $pid (@{ cgroup_member_snapshot($launch_path) }) {
        my $stat = hardened_linux_process_stat($pid) // next;
        return 1 if linux_state_is_live($stat->{state}) && $stat->{pgrp} == $pgid;
    }
    return 0;
}

sub attempt_emergency_containment {
    my ($launch_path, $pid, $group_ready, $observed) = @_;
    $observed->{emergency_attempts} < 2
        or fail('emergency containment attempt budget exhausted');
    my $started = clock_gettime(CLOCK_MONOTONIC);
    kill 'KILL', $pid;
    $observed->{direct_kill_attempted} = 1;
    if ($group_ready) {
        kill 'KILL', -$pid;
        $observed->{pgid_kill_attempted} = 1;
    }
    my $kill_path = File::Spec->catfile($launch_path, 'cgroup.kill');
    my $kill_ok = eval {
        write_control_file($kill_path, "1\n", 'cgroup.kill');
        1;
    };
    $observed->{cgroup_kill_attempted} = 1;
    unless ($kill_ok) {
        $observed->{cleanup_infrastructure} = 1;
        $observed->{infrastructure_error} //=
            trim_one_line($@, 'cgroup.kill error');
    }
    my $elapsed = int(
        (clock_gettime(CLOCK_MONOTONIC) - $started) * 1_000_000 + 0.999999);
    record_emergency_attempt($observed, $elapsed);
}

sub record_emergency_attempt {
    my ($observed, $elapsed) = @_;
    $observed->{emergency_attempts} < 2
        or fail('emergency containment attempt budget exhausted');
    ++$observed->{emergency_attempts};
    $observed->{"emergency_attempt_$observed->{emergency_attempts}_elapsed_us"} =
        $elapsed > $TERM_GRACE_US ? $TERM_GRACE_US : $elapsed;
}

sub cleanup_hardened_launch {
    my (%args) = @_;
    my $launch = $args{launch};
    my $pid = $args{pid};
    my $observed = $args{observed};
    my $force = $args{force};
    my $group_ready = $observed->{readiness_seen} && $observed->{setsid_verified};
    if ($force) {
        kill 'TERM', $pid;
        kill 'TERM', -$pid if $group_ready;
        usleep(10_000);
        attempt_emergency_containment(
            $launch->{path}, $pid, $group_ready, $observed);
    }
    my $drained = 0;
    while (1) {
        my $deadline = clock_gettime(CLOCK_MONOTONIC)
            + $DRAIN_GRACE_US / 1_000_000;
        while (clock_gettime(CLOCK_MONOTONIC) < $deadline) {
            my $events = cgroup_events($launch->{path});
            my $pgid_empty = !$group_ready
                || !pgid_has_live_cgroup_member($launch->{path}, $pid);
            if ($events->{populated} == 0 && $pgid_empty) {
                $observed->{cleanup_populated_zero} = 1;
                $observed->{cleanup_pgid_empty} = 1;
                $drained = 1;
                last;
            }
            usleep(1_000);
        }
        last if $drained;
        last if $observed->{emergency_attempts} >= 2;
        attempt_emergency_containment(
            $launch->{path}, $pid, $group_ready, $observed);
    }
    if (!$drained) {
        capture_final_cgroup_memory($launch, $observed);
        $observed->{cgroup_retained} = 1;
        $observed->{cleanup_infrastructure} = 1;
        $observed->{infrastructure_error} //= 'post-kill-drain-expired';
        return;
    }
    capture_final_cgroup_memory($launch, $observed);
    my $reaped = waitpid($pid, WNOHANG);
    if ($reaped == 0) {
        my $deadline = clock_gettime(CLOCK_MONOTONIC) + 0.05;
        while ($reaped == 0 && clock_gettime(CLOCK_MONOTONIC) < $deadline) {
            usleep(500);
            $reaped = waitpid($pid, WNOHANG);
        }
    }
    $reaped == $pid or fail("cannot reap hardened launch leader $pid");
    $observed->{wait_status} = $?;
    $observed->{leader_reaped} = 1;
    if ($args{retain_after_reap}) {
        $observed->{cgroup_retained} = 1;
        $observed->{cleanup_infrastructure} = 1;
        $observed->{cleanup} = 'retained';
        return;
    }
    rmdir $launch->{path}
        or fail("cannot remove launch cgroup $launch->{path}: $!");
    $observed->{cgroup_removed} = 1;
}

sub post_read_capture {
    my ($path, $limit, $flag, $flags, $observed) = @_;
    my ($bytes, $oversized, $count);
    my $ok = eval {
        ($bytes, $oversized, $count) = read_bounded_file($path, $limit);
        1;
    };
    if (!$ok) {
        $flags->{infrastructure} = 1;
        $observed->{infrastructure_error} //= 'bounded-post-read-failure';
        return '';
    }
    $observed->{"${flag}_bytes"} = $count;
    $flags->{$flag} = 1 if $oversized;
    return $bytes;
}

sub initial_launch_result {
    my (%args) = @_;
    my $launch = $args{launch};
    my %result = (
        kind => $args{kind} // 'fixture',
        mode => $args{mode} // 'fixture',
        termination => 'infrastructure',
        scope_unit => $args{scope}{unit},
        scope_control_group => $args{scope}{control_group},
        launch_cgroup => $launch->{path},
        cgroup_type => $launch->{cgroup_type},
        memory_max => $launch->{memory_max},
        memory_swap_max => $launch->{memory_swap_max},
        memory_oom_group => $launch->{memory_oom_group},
        rss_peak => 0,
        memory_current => $launch->{memory_current_preflight},
        memory_peak => $launch->{memory_peak_preflight},
        memory_source => 'none',
        readiness_seen => 0,
        membership_verified => 0,
        setsid_verified => 0,
        direct_kill_attempted => 0,
        pgid_kill_attempted => 0,
        cgroup_kill_attempted => 0,
        emergency_attempts => 0,
        cleanup_populated_zero => 0,
        cleanup_pgid_empty => 0,
        leader_reaped => 0,
        cgroup_removed => 0,
        cgroup_retained => 0,
        validator_launched => $args{validator_launched} // 0,
        infrastructure_error => $args{infrastructure_error},
        leader_pid => 0,
        leader_start_ticks => 0,
        exit_code => 255,
        elapsed_us => 0,
        stdout_bytes => 0,
        stderr_bytes => 0,
        trace_bytes => 0,
        max_rss_sample_interval_us => 0,
        cleanup => 'retained',
    );
    for my $family (qw(max oom oom_kill oom_group_kill)) {
        my $baseline = $launch->{events_baseline}{$family};
        $result{"events_${family}_baseline"} = $baseline;
        $result{"events_${family}_final"} = $baseline;
        $result{"events_${family}_delta"} = 0;
    }
    return \%result;
}

sub record_hardened_child_action {
    my ($handle, $sequence, $action) = @_;
    return unless defined $handle;
    my $line = "actor=child seq=$sequence action=$action\n";
    my $written = syswrite($handle, $line);
    defined $written && $written == length($line)
        or die "cannot record hardened child action: $!\n";
}

sub hardened_supervise_process_inner {
    my (%args) = @_;
    my $lifecycle = $args{lifecycle};
    my @command = @{ $args{command} // [] };
    @command or fail('hardened supervisor received an empty command');
    my $launch = configure_launch_cgroup(
        scope => $args{scope}, name => $args{launch_name},
        memory_max => $args{memory_max} // $PRODUCTION_MEMORY_MAX);
    $lifecycle->{launch} = $launch;
    $lifecycle->{observed} = initial_launch_result(
        scope => $args{scope}, launch => $launch,
        kind => $args{kind}, mode => $args{mode},
        validator_launched => $args{validator_launched});
    my ($executable, $executable_identity) = open_stable_executable(
        $args{binary}, $args{binary_identity});
    $lifecycle->{executable} = $executable;
    my $executable_fd = fileno($executable);
    defined $executable_fd or fail('stable executable has no descriptor');
    my $stdout_handle = create_capture($args{stdout_path});
    $lifecycle->{stdout_handle} = $stdout_handle;
    my $stderr_handle = create_capture($args{stderr_path});
    $lifecycle->{stderr_handle} = $stderr_handle;
    my $action_handle;
    if (defined $args{child_action_path}) {
        $action_handle = create_capture($args{child_action_path});
        $lifecycle->{action_handle} = $action_handle;
        my $header = "typokat-wu0e-preflight-action-trace-v1\n";
        my $written = syswrite($action_handle, $header);
        defined $written && $written == length($header)
            or fail("cannot initialize hardened child action trace: $!");
    }
    pipe(my $ready_reader, my $ready_writer) or fail("readiness pipe failed: $!");
    $lifecycle->{ready_reader} = $ready_reader;
    $lifecycle->{ready_writer} = $ready_writer;
    fcntl($ready_writer, F_SETFD,
        fcntl($ready_writer, F_GETFD, 0) | FD_CLOEXEC)
        or fail("cannot set readiness close-on-exec: $!");
    fcntl($ready_reader, F_SETFL,
        fcntl($ready_reader, F_GETFL, 0) | O_NONBLOCK)
        or fail("cannot make readiness pipe nonblocking: $!");

    my $started = clock_gettime(CLOCK_MONOTONIC);
    $lifecycle->{started} = $started;
    my $pid = fork();
    defined $pid or fail("fork failed for hardened launch: $!");
    $lifecycle->{pid} = $pid if $pid != 0;
    if ($pid == 0) {
        my $child_ok = eval {
            write_control_file(
                File::Spec->catfile($launch->{path}, 'cgroup.procs'), "$$\n",
                'child launch membership');
            record_hardened_child_action($action_handle, 0, 'self-move');
            close $ready_reader;
            open STDOUT, '>&', $stdout_handle
                or die "cannot redirect stdout: $!\n";
            open STDERR, '>&', $stderr_handle
                or die "cannot redirect stderr: $!\n";
            setsid() >= 0 or die "setsid failed: $!\n";
            record_hardened_child_action($action_handle, 1, 'setsid');
            syswrite($ready_writer, 'R') == 1
                or die "cannot publish hardened readiness: $!\n";
            record_hardened_child_action($action_handle, 2, 'readiness');
            close $ready_writer or die "cannot close hardened readiness: $!\n";
            scrub_wu_environment($args{environment} // {});
            record_hardened_child_action($action_handle, 3, 'environment');
            $args{child_setup}->() if $args{child_setup};
            my $fd_path = "/proc/self/fd/$executable_fd";
            record_hardened_child_action($action_handle, 4, 'stable-exec');
            if (defined $action_handle) {
                close $action_handle
                    or die "cannot close hardened child action trace: $!\n";
            }
            exec { $fd_path } @command;
            die "cannot exec stable handle for $command[0]: $!\n";
        };
        if (!$child_ok) {
            my $error = $@ || "unknown hardened child failure\n";
            syswrite($stderr_handle, $error);
            POSIX::_exit(127);
        }
        POSIX::_exit(127);
    }
    close $stdout_handle or fail("cannot close hardened stdout capture: $!");
    delete $lifecycle->{stdout_handle};
    close $stderr_handle or fail("cannot close hardened stderr capture: $!");
    delete $lifecycle->{stderr_handle};
    if (defined $action_handle) {
        close $action_handle
            or fail("cannot close parent hardened child action trace: $!");
        delete $lifecycle->{action_handle};
    }
    close $ready_writer;
    delete $lifecycle->{ready_writer};
    close $executable or fail("cannot close parent stable executable: $!");
    delete $lifecycle->{executable};
    verify_stable_executable_path($args{binary}, $executable_identity);

    my $leader = hardened_linux_process_stat($pid)
        // fail("cannot inspect hardened leader $pid");
    my %observed = %{ $lifecycle->{observed} };
    $observed{leader_pid} = $pid;
    $observed{leader_start_ticks} = $leader->{start_ticks};
    $observed{checked_files} = join(',', @{ $launch->{checked_files} });
    $observed{cgroup_kill_access} = $launch->{cgroup_kill_access};
    $lifecycle->{observed} = \%observed;
    if (defined $args{inject_outer_exception_after_fork}) {
        $lifecycle->{retain_after_reap} = 1;
        $lifecycle->{exception_phase} = 'post-fork';
        $args{retained_event_callback}->(
            'outer-exception', $args{inject_outer_exception_after_fork})
            if defined $args{retained_event_callback};
        die "$args{inject_outer_exception_after_fork}\n";
    }
    my %flags = map { $_ => 0 }
        qw(infrastructure trace stdout stderr rss deadline crash);
    my ($readiness_closed, $last_sample_at, $sample_delayed, $force_cleanup) =
        (0, undef, 0, 0);
    my $launch_callback_ran = 0;
    my $monitor_ok = eval {
        while (1) {
            unless ($observed{readiness_seen}) {
                my $byte = '';
                my $count = sysread($ready_reader, $byte, 1);
                if (defined $count && $count == 1) {
                    $byte eq 'R' or fail('invalid hardened readiness byte');
                    my @members = cgroup_members($launch->{path});
                    scalar(grep { $_ == $pid } @members) == 1
                        or fail('hardened readiness preceded cgroup membership');
                    my $ready_stat = hardened_linux_process_stat($pid)
                        // fail('hardened readiness leader disappeared');
                    $ready_stat->{pgrp} == $pid
                        or fail('hardened readiness preceded setsid');
                    $observed{readiness_seen} = 1;
                    $observed{membership_verified} = 1;
                    $observed{setsid_verified} = 1;
                    if (defined $args{launch_confirmed_callback}) {
                        $launch_callback_ran = 1;
                        $args{launch_confirmed_callback}->();
                    }
                } elsif (defined $count && $count == 0) {
                    $readiness_closed = 1;
                } elsif (!defined $count && $! != EAGAIN && $! != EWOULDBLOCK) {
                    fail("cannot read hardened readiness: $!");
                }
            }
            $readiness_closed && !$observed{readiness_seen}
                and fail('hardened child closed readiness before confirmation');
            if ($observed{readiness_seen} && defined $args{inject_monitor_exception}) {
                die "$args{inject_monitor_exception}\n";
            }
            my $now = clock_gettime(CLOCK_MONOTONIC);
            if ($observed{readiness_seen}) {
                if (defined $last_sample_at) {
                    my $interval = int(($now - $last_sample_at) * 1_000_000 + 0.999999);
                    $observed{max_rss_sample_interval_us} = $interval
                        if $interval > $observed{max_rss_sample_interval_us};
                }
                $last_sample_at = $now;
                my ($sum, undef, undef, $rss_error) =
                    sample_cgroup_rss($launch->{path});
                defined $rss_error and fail($rss_error);
                $observed{rss_peak} = $sum if $sum > $observed{rss_peak};
                $flags{rss} = 1
                    if $sum > ($args{rss_limit} // $MAX_PROCESS_GROUP_RSS_BYTES);
                if (!$sample_delayed && $args{sample_delay_us}) {
                    $sample_delayed = 1;
                    usleep($args{sample_delay_us});
                }
            }
            my $stdout_bytes = file_size($args{stdout_path}, 0);
            my $stderr_bytes = file_size($args{stderr_path}, 0);
            my $trace_bytes = defined $args{trace_path}
                ? file_size($args{trace_path}, 1) : 0;
            $flags{stdout} = 1
                if $stdout_bytes > ($args{stdout_limit} // $MAX_STDOUT_BYTES);
            $flags{stderr} = 1
                if $stderr_bytes > ($args{stderr_limit} // $MAX_STDERR_BYTES);
            $flags{trace} = 1
                if $trace_bytes > ($args{trace_limit} // $MAX_TRACE_BYTES);
            $flags{deadline} = 1
                if ($now - $started) * 1_000_000
                    >= ($args{deadline_us} // $DEADLINE_US);
            my $events = memory_events_local($launch->{path});
            my $causal_memory = grep {
                memory_event_delta($launch->{events_baseline}, $events, $_) > 0
            } qw(oom oom_kill oom_group_kill);
            $flags{rss} = 1 if $causal_memory;
            if (grep { $flags{$_} } qw(trace stdout stderr rss deadline)) {
                $force_cleanup = 1;
                last;
            }
            my $current = hardened_linux_process_stat($pid);
            if (defined $current && !linux_state_is_live($current->{state})) {
                my $cg_events = cgroup_events($launch->{path});
                last if $cg_events->{populated} == 0
                    && !pgid_has_live_cgroup_member($launch->{path}, $pid);
            }
            usleep($RSS_SAMPLE_TARGET_US);
        }
        1;
    };
    if (!$monitor_ok) {
        my $error = $@;
        $error =~ s/\n\z//;
        $error =~ s/\Awu0e-diagnostic: //;
        $flags{infrastructure} = 1;
        $observed{infrastructure_error} //= $error || 'monitor-exception';
        $force_cleanup = 1;
    }
    close $ready_reader or do {
        $flags{infrastructure} = 1;
        $observed{infrastructure_error} //= 'readiness-close-failure';
        $force_cleanup = 1;
    };
    delete $lifecycle->{ready_reader};

    my $cleanup_ok = eval {
        cleanup_hardened_launch(
            launch => $launch, pid => $pid, observed => \%observed,
            force => $force_cleanup || $flags{infrastructure});
        1;
    };
    if (!$cleanup_ok) {
        my $error = $@;
        $error =~ s/\n\z//;
        $error =~ s/\Awu0e-diagnostic: //;
        $flags{infrastructure} = 1;
        $observed{infrastructure_error} //= $error || 'cleanup-exception';
        $observed{cgroup_retained} = 1 unless $observed{cgroup_removed};
        $observed{cleanup_infrastructure} = 1;
    }
    if (!defined $observed{memory_source} && -d $launch->{path}) {
        my $metrics_ok = eval {
            capture_final_cgroup_memory($launch, \%observed);
            1;
        };
        if (!$metrics_ok) {
            $flags{infrastructure} = 1;
            $observed{infrastructure_error} //= 'final-memory-metadata-failure';
        }
    }
    $flags{rss} = 1
        if defined $observed{memory_source}
            && $observed{memory_source} =~ /\Aoom(?:_|\z)/;
    $flags{infrastructure} = 1
        if $observed{cleanup_infrastructure} || $observed{cgroup_retained};
    my $stdout = post_read_capture(
        $args{stdout_path}, $args{stdout_limit} // $MAX_STDOUT_BYTES,
        'stdout', \%flags, \%observed);
    my $stderr = post_read_capture(
        $args{stderr_path}, $args{stderr_limit} // $MAX_STDERR_BYTES,
        'stderr', \%flags, \%observed);
    if (defined $args{trace_path}) {
        post_read_capture(
            $args{trace_path}, $args{trace_limit} // $MAX_TRACE_BYTES,
            'trace', \%flags, \%observed);
    } else {
        $observed{trace_bytes} = 0;
    }
    my $exit_code = defined $observed{wait_status}
        ? decode_wait_status($observed{wait_status}) : undef;
    $flags{crash} = 1 if defined $exit_code && $exit_code != 0;
    if (defined $args{post_infrastructure_error}) {
        $flags{infrastructure} = 1;
        $observed{infrastructure_error} = $args{post_infrastructure_error};
    }
    $observed{exit_code} = defined $exit_code ? $exit_code : 255;
    $observed{termination} = adjudicate_termination(\%flags, $exit_code);
    $observed{elapsed_us} = int(
        (clock_gettime(CLOCK_MONOTONIC) - $started) * 1_000_000 + 0.999999);
    $observed{infrastructure_error} //= 'none';
    $observed{launch_callback_ran} = $launch_callback_ran;
    $observed{cleanup} = $observed{cgroup_removed} ? 'removed' : 'retained';
    $observed{_executable_identity} = $executable_identity;
    $lifecycle->{completed} = 1;
    return (\%observed, $stdout, $stderr);
}

sub cleanup_failed_hardened_lifecycle {
    my ($lifecycle) = @_;
    for my $key (qw(
        ready_reader ready_writer stdout_handle stderr_handle action_handle
        executable
    )) {
        next unless defined $lifecycle->{$key};
        close $lifecycle->{$key};
        delete $lifecycle->{$key};
    }
    my $launch = $lifecycle->{launch};
    return unless defined $launch;
    if (!defined $lifecycle->{pid}) {
        if (-d $launch->{path}) {
            rmdir $launch->{path}
                or fail("cannot remove failed pre-fork launch cgroup $launch->{path}: $!");
        }
        return;
    }
    my $pid = $lifecycle->{pid};
    if (!-d $launch->{path}) {
        my $stat = hardened_linux_process_stat($pid);
        kill 'KILL', $pid;
        kill 'KILL', -$pid
            if ($lifecycle->{observed}{setsid_verified} // 0)
                || (defined $stat && $stat->{pgrp} == $pid);
        my $deadline = clock_gettime(CLOCK_MONOTONIC) + 0.25;
        my $reaped = waitpid($pid, WNOHANG);
        while ($reaped == 0 && clock_gettime(CLOCK_MONOTONIC) < $deadline) {
            usleep(1_000);
            $reaped = waitpid($pid, WNOHANG);
        }
        $reaped == $pid
            or fail('launch cgroup disappeared and child could not be reaped');
        fail('launch cgroup disappeared during post-fork cleanup');
    }
    my $observed = $lifecycle->{observed} // {
        readiness_seen => 0, setsid_verified => 0,
        direct_kill_attempted => 0, pgid_kill_attempted => 0,
        cgroup_kill_attempted => 0, emergency_attempts => 0,
        cleanup_populated_zero => 0, cleanup_pgid_empty => 0,
        leader_reaped => 0, cgroup_removed => 0, cgroup_retained => 0,
        infrastructure_error => 'parent-launch-exception',
    };
    cleanup_hardened_launch(
        launch => $launch, pid => $pid, observed => $observed, force => 1,
        retain_after_reap => $lifecycle->{retain_after_reap});
    $observed->{cgroup_removed} || $observed->{cgroup_retained}
        or fail('failed lifecycle cleanup reached no terminal cgroup state');
    return $observed;
}

sub one_line_infrastructure_error {
    my (@errors) = @_;
    my @parts;
    for my $error (@errors) {
        next unless defined $error && $error ne '';
        $error =~ s/\Awu0e-diagnostic: //;
        $error =~ s/[\r\n]+/; /g;
        $error =~ s/; \z//;
        push @parts, $error if $error ne '';
    }
    return @parts ? join(' | ', @parts) : 'retained-lifecycle-exception';
}

sub failed_lifecycle_result {
    my (%args) = @_;
    my $lifecycle = $args{lifecycle};
    my $launch = $lifecycle->{launch};
    my $result = initial_launch_result(
        scope => $args{scope}, launch => $launch,
        kind => $args{kind}, mode => $args{mode},
        validator_launched => $args{validator_launched});
    %$result = (%$result, %{ $lifecycle->{observed} // {} });
    if (defined $lifecycle->{pid}) {
        $result->{leader_pid} = $lifecycle->{pid};
        if (!$result->{leader_start_ticks}) {
            my $stat = hardened_linux_process_stat($lifecycle->{pid});
            $result->{leader_start_ticks} = $stat->{start_ticks}
                if defined $stat;
        }
    }
    if (-d $launch->{path}) {
        my $metrics_ok = eval {
            capture_final_cgroup_memory($launch, $result);
            1;
        };
        $args{cleanup_error} .= $@ unless $metrics_ok;
    }
    for my $capture (
        ['stdout_bytes', $args{stdout_path}, 0],
        ['stderr_bytes', $args{stderr_path}, 0],
        ['trace_bytes', $args{trace_path}, 1],
    ) {
        my ($field, $path, $may_be_missing) = @$capture;
        next unless defined $path;
        my $size_ok = eval {
            $result->{$field} = file_size($path, $may_be_missing);
            1;
        };
        $args{cleanup_error} .= $@ unless $size_ok;
    }
    $result->{termination} = 'infrastructure';
    $result->{cgroup_retained} = 1;
    $result->{cgroup_removed} = 0;
    $result->{cleanup} = 'retained';
    $result->{scope_abort_requested} = 1;
    $result->{exception_phase} = $lifecycle->{exception_phase}
        if defined $lifecycle->{exception_phase};
    $result->{infrastructure_error} = one_line_infrastructure_error(
        $args{primary_error}, $args{cleanup_error});
    $result->{elapsed_us} = defined $lifecycle->{started}
        ? int((clock_gettime(CLOCK_MONOTONIC) - $lifecycle->{started})
            * 1_000_000 + 0.999999)
        : 0;
    $result->{exit_code} = defined $result->{wait_status}
        ? decode_wait_status($result->{wait_status}) : 255;
    return $result;
}

sub persist_retained_failure_and_abort {
    my (%args) = @_;
    my $result = $args{result};
    my $meta_path = $args{meta_path}
        // "$args{stderr_path}.retained.process-meta";
    $result->{meta_fsync_completed} = 1;
    write_process_meta_v2($meta_path, $result, {});
    $args{retained_event_callback}->(
        'process-meta-fsynced', $meta_path, $result)
        if defined $args{retained_event_callback};
    $args{scope_abort_requested_callback}->()
        if defined $args{scope_abort_requested_callback};
    my %abort = (scope => $args{scope});
    $abort{injected_request_callback} = $args{scope_abort_request_callback}
        if defined $args{scope_abort_request_callback};
    $abort{identity_verified_callback} = sub {
        my ($unit, $control_group) = @_;
        $args{retained_event_callback}->(
            'scope-identity-reverified', $unit, $control_group)
            if defined $args{retained_event_callback};
    };
    $abort{request_observed_callback} = sub {
        my ($command) = @_;
        $args{retained_event_callback}->(
            'scope-abort-requested', $command)
            if defined $args{retained_event_callback};
    };
    request_verified_scope_abort(%abort);
}

sub hardened_supervise_process {
    my (%args) = @_;
    my %lifecycle;
    my @result;
    my $ok = eval {
        @result = hardened_supervise_process_inner(
            %args, lifecycle => \%lifecycle);
        1;
    };
    if ($ok) {
        if ($result[0]{cgroup_retained}) {
            $result[0]{scope_abort_requested} = 1;
            persist_retained_failure_and_abort(%args, result => $result[0]);
            fail('delegated scope abort request returned after retained launch');
        }
        return @result;
    }
    my $primary_error = $@;
    if (defined $lifecycle{launch} && defined $lifecycle{pid}
        && !defined $lifecycle{observed}) {
        $lifecycle{observed} = initial_launch_result(
            scope => $args{scope}, launch => $lifecycle{launch},
            kind => $args{kind}, mode => $args{mode},
            validator_launched => $args{validator_launched});
    }
    my $cleanup_error = '';
    my $cleanup_ok = eval {
        cleanup_failed_hardened_lifecycle(\%lifecycle)
            unless $lifecycle{completed};
        1;
    };
    $cleanup_error = $@ unless $cleanup_ok;
    my $retained = defined $lifecycle{launch}
        && (-d $lifecycle{launch}{path}
            || (($lifecycle{observed} // {})->{cgroup_retained} // 0));
    if ($retained) {
        my $failed = failed_lifecycle_result(
            %args, lifecycle => \%lifecycle,
            primary_error => $primary_error,
            cleanup_error => $cleanup_error);
        persist_retained_failure_and_abort(%args, result => $failed);
        $args{retained_event_callback}->(
            'outer-exception-propagated',
            one_line_infrastructure_error($primary_error))
            if defined $args{retained_event_callback};
        die $primary_error;
    }
    die $primary_error . $cleanup_error;
}

sub write_bytes_exclusive_fsynced {
    my ($path, $bytes) = @_;
    sysopen my $handle, $path, O_WRONLY | O_CREAT | O_EXCL | O_NOFOLLOW, 0600
        or fail("cannot create $path: $!");
    binmode $handle, ':raw';
    print {$handle} $bytes or fail("cannot write $path: $!");
    $handle->flush() or fail("cannot flush $path: $!");
    $handle->sync() or fail("cannot fsync $path: $!");
    close $handle or fail("cannot close $path: $!");
}

sub process_meta_v2_bytes {
    my ($result, $extra) = @_;
    my @keys = qw(
        kind mode termination scope_unit scope_control_group launch_cgroup
        cgroup_type memory_max memory_swap_max memory_oom_group rss_peak
        memory_current memory_peak events_max_baseline events_max_final
        events_max_delta events_oom_baseline events_oom_final events_oom_delta
        events_oom_kill_baseline events_oom_kill_final events_oom_kill_delta
        events_oom_group_kill_baseline events_oom_group_kill_final
        events_oom_group_kill_delta memory_source readiness_seen
        membership_verified setsid_verified direct_kill_attempted
        pgid_kill_attempted cgroup_kill_attempted emergency_attempts
        cleanup_populated_zero cleanup_pgid_empty leader_reaped cgroup_removed
        cgroup_retained validator_launched infrastructure_error leader_pid
        leader_start_ticks exit_code elapsed_us stdout_bytes stderr_bytes
        trace_bytes max_rss_sample_interval_us cleanup
    );
    my %fields = (%$result, %{$extra // {}});
    my %base_key = map { $_ => 1 } @keys;
    my @extra_keys = sort grep { !$base_key{$_} } keys %fields;
    my %written;
    my @lines = ('typokat-wu0e-process-meta-v2');
    for my $key (@keys, @extra_keys) {
        next if $written{$key}++;
        exists $fields{$key} or fail("process metadata lacks $key");
        my $value = $fields{$key};
        defined $value && "$value" ne '' && "$value" !~ /[\r\n]/
            or fail("invalid process metadata value for $key");
        push @lines, "$key=$value";
    }
    return join("\n", @lines) . "\n";
}

sub write_process_meta_v2 {
    my ($path, $result, $extra) = @_;
    write_bytes_exclusive_fsynced($path, process_meta_v2_bytes($result, $extra));
}

sub validate_semantic_parity_v2 {
    my ($observations) = @_;
    my @completed = grep { $_->{semantic_sha256} ne 'unavailable' } @$observations;
    return if @completed < 2;
    my $first = $completed[0];
    for my $other (@completed[1 .. $#completed]) {
        $other->{semantic_sha256} eq $first->{semantic_sha256}
            or fail("completed semantic mismatch: $first->{mode}=$first->{semantic_sha256} $other->{mode}=$other->{semantic_sha256}");
    }
}

sub dossier_v2_bytes {
    my (%args) = @_;
    validate_semantic_parity_v2($args{observations});
    my @lines = (
        'typokat-wu0e-diagnostic-dossier-v2',
        "binary_identity=$args{binary_identity}",
        "host_identity=$args{host_identity}",
        "profile_identity=$args{profile_identity}",
        "inventory_identity=$args{inventory_identity}",
        'mode_order=plain,measured-off,candidate-b',
    );
    for my $observation (@{ $args{observations} }) {
        my @fields = ('workload', "mode=$observation->{mode}");
        for my $key (qw(
            termination semantic_sha256 scope_unit scope_control_group
            launch_cgroup memory_max memory_swap_max memory_oom_group rss_peak
            memory_current memory_peak events_max_baseline events_max_final
            events_max_delta events_oom_baseline events_oom_final events_oom_delta
            events_oom_kill_baseline events_oom_kill_final events_oom_kill_delta
            events_oom_group_kill_baseline events_oom_group_kill_final
            events_oom_group_kill_delta memory_source readiness membership setsid
            direct_kill_attempted pgid_kill_attempted cgroup_kill_attempted
            cleanup_populated_zero cleanup_pgid_empty leader_reaped cgroup_removed
            cgroup_retained cleanup
        )) {
            exists $observation->{$key}
                or fail("dossier observation lacks $key");
            push @fields, "$key=$observation->{$key}";
        }
        push @lines, join ' ', @fields;
    }
    return join("\n", @lines) . "\n";
}

sub hardening_dossier_fixture {
    my ($scope) = @_;
    my $a = 'a' x 64;
    my $base = {
        scope_unit => 'fixture.scope', scope_control_group => '/fixture.scope',
        memory_max => $PRODUCTION_MEMORY_MAX, memory_swap_max => 0,
        memory_oom_group => 1, rss_peak => 4096, memory_current => 4096,
        memory_peak => 8192, events_max_baseline => 0, events_max_final => 0,
        events_max_delta => 0, events_oom_baseline => 0, events_oom_final => 0,
        events_oom_delta => 0, events_oom_kill_baseline => 0,
        events_oom_kill_final => 0, events_oom_kill_delta => 0,
        events_oom_group_kill_baseline => 0, events_oom_group_kill_final => 0,
        events_oom_group_kill_delta => 0, memory_source => 'none',
        readiness => 1, membership => 1, setsid => 1,
        cleanup_populated_zero => 1, cleanup_pgid_empty => 1,
        leader_reaped => 1, cgroup_removed => 1, cgroup_retained => 0,
        cleanup => 'removed',
    };
    my @observations = (
        { %$base, mode => 'plain', termination => 'normal',
            semantic_sha256 => $a, launch_cgroup => '/fixture.scope/plain',
            direct_kill_attempted => 0, pgid_kill_attempted => 0,
            cgroup_kill_attempted => 0 },
        { %$base, mode => 'measured-off', termination => 'normal',
            semantic_sha256 => $a,
            launch_cgroup => '/fixture.scope/measured-off',
            direct_kill_attempted => 0, pgid_kill_attempted => 0,
            cgroup_kill_attempted => 0 },
        { %$base, mode => 'candidate-b', termination => 'deadline',
            semantic_sha256 => 'unavailable',
            launch_cgroup => '/fixture.scope/candidate-b',
            direct_kill_attempted => 1, pgid_kill_attempted => 1,
            cgroup_kill_attempted => 1 },
    );
    return dossier_v2_bytes(
        binary_identity => 'c' x 64, host_identity => 'd' x 64,
        profile_identity => 'e' x 64, inventory_identity => 'f' x 64,
        observations => \@observations);
}

sub hardening_termination_fixture_bytes {
    my @cases = (
        ['all-loop', [qw(infrastructure trace stdout stderr rss deadline crash)], [], undef],
        ['post-infrastructure', [qw(trace stdout stderr rss deadline crash)], ['infrastructure'], undef],
        ['post-trace', [qw(stdout stderr rss deadline crash)], ['trace'], undef],
        ['post-stdout', [qw(stderr rss deadline crash)], ['stdout'], undef],
        ['post-stderr', [qw(rss deadline crash)], ['stderr'], undef],
        ['post-rss', [qw(deadline crash)], ['rss'], undef],
        ['deadline', [qw(deadline crash)], [], undef],
        ['crash', [qw(crash)], [], undef],
        ['normal', [], [], 0],
    );
    my @lines = ('typokat-wu0e-termination-fixtures-v1');
    for my $case (@cases) {
        my ($name, $loop, $post, $exit_code) = @$case;
        my %flags = map { $_ => 0 }
            qw(infrastructure trace stdout stderr rss deadline crash);
        $flags{$_} = 1 for @$loop, @$post;
        my $actual = adjudicate_termination(\%flags, $exit_code);
        push @lines, join ' ', "case=$name",
            'flags=' . (@$loop ? join(',', @$loop) : 'none'),
            'post=' . (@$post ? join(',', @$post) : 'none'), "actual=$actual";
    }
    my %normal = map { $_ => 0 }
        qw(infrastructure trace stdout stderr rss deadline crash);
    push @lines,
        'case=delayed-rss-sample flags=none post=none sample_interval_us=15000 target_us=10000 actual='
            . adjudicate_termination(\%normal, 0),
        'case=max-contact flags=memory-max-contact post=none actual='
            . adjudicate_termination(\%normal, 0) . ' memory_source=max';
    return join("\n", @lines) . "\n";
}

sub hardening_preflight_failure_bytes {
    my %passing = (
        cgroup_available => 1,
        delegate => 'yes',
        memory_controller_available => 1,
        cgroup_type_available => 1,
        cgroup_type => 'domain',
        cgroup_procs_accessible => 1,
        cgroup_events_valid => 1,
        cgroup_kill_writable => 1,
        memory_max_requested => $PRODUCTION_MEMORY_MAX,
        memory_max_readback => $PRODUCTION_MEMORY_MAX,
        memory_swap_max_readback => 0,
        memory_oom_group_readback => 1,
        memory_current_available => 1,
        memory_peak_valid => 1,
        memory_events_local_readable => 1,
    );
    my @cases = (
        ['cgroup-unavailable', sub { $_[0]{cgroup_available} = 0 }],
        ['delegate-false', sub { $_[0]{delegate} = 'no' }],
        ['memory-controller-missing',
            sub { $_[0]{memory_controller_available} = 0 }],
        ['cgroup-type-missing', sub { $_[0]{cgroup_type_available} = 0 }],
        ['cgroup-type-threaded', sub { $_[0]{cgroup_type} = 'threaded' }],
        ['cgroup-procs-inaccessible',
            sub { $_[0]{cgroup_procs_accessible} = 0 }],
        ['cgroup-events-malformed', sub { $_[0]{cgroup_events_valid} = 0 }],
        ['cgroup-kill-unwritable', sub { $_[0]{cgroup_kill_writable} = 0 }],
        ['memory-max-readback-mismatch',
            sub { $_[0]{memory_max_readback} = $PRODUCTION_MEMORY_MAX - 1 }],
        ['memory-swap-max-readback-mismatch',
            sub { $_[0]{memory_swap_max_readback} = 1 }],
        ['memory-oom-group-readback-mismatch',
            sub { $_[0]{memory_oom_group_readback} = 0 }],
        ['memory-current-missing',
            sub { $_[0]{memory_current_available} = 0 }],
        ['memory-peak-malformed', sub { $_[0]{memory_peak_valid} = 0 }],
        ['memory-events-local-unreadable',
            sub { $_[0]{memory_events_local_readable} = 0 }],
    );
    my @lines = ('typokat-wu0e-preflight-failure-fixtures-v1');
    for my $case (@cases) {
        my ($name, $mutate) = @$case;
        my %view = %passing;
        $mutate->(\%view);
        my ($workload_exec, $validator_exec) = (0, 0);
        my $admission = evaluate_preflight_admission(
            view => \%view,
            workload_callback => sub { ++$workload_exec },
            validator_callback => sub { ++$validator_exec });
        $admission->{termination} eq 'infrastructure'
            && $admission->{failure} eq $name
            or fail("injected preflight view was misclassified: $name");
        push @lines,
            "case=$name actual=$admission->{termination} workload_exec=$workload_exec validator_exec=$validator_exec";
    }
    my ($workload_exec, $validator_exec) = (0, 0);
    my $passing_admission = evaluate_preflight_admission(
        view => \%passing,
        workload_callback => sub { ++$workload_exec },
        validator_callback => sub { ++$validator_exec });
    $passing_admission->{termination} eq 'admitted'
        && $workload_exec == 1 && $validator_exec == 1
        or fail('passing preflight view did not reach launch callbacks');
    return join("\n", @lines) . "\n";
}

sub hardening_rss_churn_fixture_bytes {
    linux_state_is_live('t') or fail('lowercase t must be live');
    !linux_state_is_live('x') or fail('lowercase x must be dead');
    my @cases = (
        ['vanished-member', [
            { status => 'retry', problem => 'vanished cgroup member 11' },
            { status => 'complete', sum => 8192, largest => 4096,
                members => 2, problem => 'none' },
        ], 'complete'],
        ['stable-unreadable-member', [
            map { { status => 'retry',
                problem => 'stably unreadable cgroup member 12' } } 1 .. 3
        ], 'infrastructure'],
        ['unresolved-membership-churn', [
            map { { status => 'retry',
                problem => 'unresolved cgroup membership churn' } } 1 .. 3
        ], 'infrastructure'],
    );
    my @lines = (
        "typokat-wu0e-rss-churn-fixtures-v1 retry_attempts=$RSS_RETRY_ATTEMPTS retry_deadline_us=$RSS_RETRY_DEADLINE_US");
    for my $case (@cases) {
        my ($name, $attempt_views, $expected_status) = @$case;
        my $next = 0;
        my $decision = execute_rss_retry_policy(
            attempt_callback => sub {
                $next < @$attempt_views
                    or fail("RSS churn fixture exhausted its attempt views: $name");
                return { %{ $attempt_views->[$next++] } };
            },
            clock_callback => sub { 0 },
            sleep_callback => sub { return });
        $decision->{status} eq $expected_status
            or fail("RSS churn fixture was misclassified: $name");
        my @fields = ("case=$name");
        for my $entry (@{ $decision->{journal} }) {
            push @fields, "attempt=$entry->{attempt}",
                "result=$entry->{result}";
        }
        push @fields, "members=$decision->{members}"
            if $decision->{status} eq 'complete';
        push @lines, join ' ', @fields;
    }
    push @lines, 'case=lowercase-t result=live',
        'case=lowercase-x result=dead';
    return join("\n", @lines) . "\n";
}

sub hardening_linux_state_fixture_bytes {
    my $t = linux_state_is_live('t') ? 'live' : 'dead';
    my $x = linux_state_is_live('x') ? 'live' : 'dead';
    return "typokat-wu0e-linux-state-fixtures-v1\ninput=t output=$t\ninput=x output=$x\n";
}

sub run_append_callback {
    my (%args) = @_;
    my $pid = fork();
    defined $pid or fail("cannot fork evidence callback: $!");
    if ($pid == 0) {
        local %ENV = %ENV;
        delete $ENV{TYPOKAT_WU0E_CALLBACK_EXIT};
        delete $ENV{TYPOKAT_WU0E_REQUIRE_FILE};
        $ENV{TYPOKAT_WU0E_CALLBACK_EXIT} = $args{exit_code}
            if $args{exit_code};
        $ENV{TYPOKAT_WU0E_REQUIRE_FILE} = $args{require_file};
        open STDIN, '<', '/dev/null' or die "cannot open /dev/null: $!\n";
        open STDOUT, '>', '/dev/null' or die "cannot open /dev/null: $!\n";
        open STDERR, '>', '/dev/null' or die "cannot open /dev/null: $!\n";
        exec { $args{probe} } $args{probe}, $args{sink}, $args{line};
        die "cannot exec append callback: $!\n";
    }
    waitpid($pid, 0) == $pid or fail('cannot reap evidence callback');
    return decode_wait_status($?);
}

sub run_scope_abort_spy {
    my (%args) = @_;
    my @systemctl_command = @{ $args{systemctl_command} };
    my @command = (
        $args{spy}, $args{sink}, $args{meta_path}, $args{unit},
        $args{control_group}, @systemctl_command);
    my $pid = fork();
    defined $pid or fail("cannot fork scope abort spy: $!");
    if ($pid == 0) {
        open STDIN, '<', '/dev/null' or die "cannot open /dev/null: $!\n";
        open STDOUT, '>', '/dev/null' or die "cannot open /dev/null: $!\n";
        open STDERR, '>', '/dev/null' or die "cannot open /dev/null: $!\n";
        exec { $command[0] } @command;
        die "cannot exec scope abort spy: $!\n";
    }
    waitpid($pid, 0) == $pid or fail('cannot reap scope abort spy');
    decode_wait_status($?) == 0 or fail('scope abort spy rejected request');
    my ($actual, $oversized) = read_bounded_file($args{sink}, 4 * 1024);
    my $expected = "callback=1 unit=$args{unit} "
        . "control_group=$args{control_group} argv="
        . join('|', @systemctl_command) . "\n";
    !$oversized && $actual eq $expected
        or fail('scope abort spy observation changed');
}

sub run_production_hook_probe {
    my (%args) = @_;
    my @command = (
        $args{probe}, $args{sink}, $args{seed}, @{ $args{fields} });
    my $pid = fork();
    defined $pid or fail("cannot fork production hook probe: $!");
    if ($pid == 0) {
        open STDIN, '<', '/dev/null' or die "cannot open /dev/null: $!\n";
        open STDOUT, '>', '/dev/null' or die "cannot open /dev/null: $!\n";
        open STDERR, '>', '/dev/null' or die "cannot open /dev/null: $!\n";
        exec { $command[0] } @command;
        die "cannot exec production hook probe: $!\n";
    }
    waitpid($pid, 0) == $pid or fail('cannot reap production hook probe');
    decode_wait_status($?) == 0 or fail('production hook probe rejected launch');
}

sub production_hook_launch {
    my (%args) = @_;
    my $contract = diagnostic_launch_contract(
        kind => $args{kind}, mode => $args{mode}, binary => $args{binary},
        trace_path => $args{trace_path}, termination => 'normal',
        binary_identity => $args{binary_identity},
        host_identity => $args{host_identity},
        profile_identity => $args{profile_identity},
        inventory_identity => $args{inventory_identity});
    my $prefix = File::Spec->catfile(
        $args{scratch}, "production-hook-$args{kind}-$args{mode}");
    my ($result) = hardened_supervise_process(
        scope => $args{scope},
        launch_name => "fixture-production-hook-$args{kind}-$args{mode}",
        kind => $args{kind}, mode => $args{mode},
        binary => $args{binary}, binary_identity => $args{binary_identity},
        command => $contract->{command}, environment => $contract->{environment},
        stdout_path => "$prefix.stdout", stderr_path => "$prefix.stderr",
        meta_path => "$prefix.retained.process-meta",
        launch_confirmed_callback => sub {
            run_production_hook_probe(
                probe => $args{probe}, sink => $args{sink}, seed => $args{seed},
                fields => [
                    "seq=$args{sequence}", 'hook=production-launch',
                    "kind=$args{kind}", "mode=$args{mode}",
                    "argv=$contract->{argv_text}",
                    "env=$contract->{environment_text}",
                    "identities=$contract->{identity_text}",
                    'preflight=admitted', 'launch=confirmed',
                ]);
        });
    $result->{termination} eq 'normal' && $result->{exit_code} == 0
        && $result->{readiness_seen} && $result->{membership_verified}
        && $result->{setsid_verified}
        or fail('production hook fixture launch did not complete normally');
    verify_completed_frozen_path(
        $args{binary}, $args{binary_identity}, $result);
    unlink "$prefix.stdout" or fail('cannot remove production hook stdout');
    unlink "$prefix.stderr" or fail('cannot remove production hook stderr');
    return $result;
}

sub run_production_hook_routing_fixture {
    my (%args) = @_;
    my $sequence = 0;
    my $schedule = run_shared_mode_scheduler(
        workload => sub {
            my ($mode) = @_;
            my $result = production_hook_launch(
                %args, kind => 'workload', mode => $mode,
                sequence => $sequence++,
                trace_path => File::Spec->catfile(
                    $args{scratch}, "production-hook-$mode.trace"));
            return $result;
        },
        validator => sub {
            my ($mode) = @_;
            production_hook_launch(
                %args, kind => 'validator', mode => $mode,
                sequence => $sequence++,
                trace_path => File::Spec->catfile(
                    $args{scratch}, "production-hook-$mode.trace"));
            return { status => 'complete' };
        },
        stop => sub { fail('production hook fixture schedule stopped') });
    !$schedule->{stopped} && $sequence == 6
        or fail('production hook fixture schedule changed');
    my ($journal, $oversized) = read_bounded_file($args{sink}, 64 * 1024);
    !$oversized or fail('production hook fixture journal exceeded bound');
    write_bytes_exclusive($args{journal_path}, $journal);
}

sub schedule_callback_line {
    my (%args) = @_;
    my $contract = diagnostic_launch_contract(
        kind => $args{kind}, mode => $args{mode},
        binary => $args{binary}, trace_path => $args{trace_path},
        termination => 'normal', binary_identity => $args{binary_identity},
        host_identity => $args{host_identity},
        profile_identity => $args{profile_identity},
        inventory_identity => $args{inventory_identity});
    return "callback seq=$args{seq} kind=$args{kind} mode=$args{mode} "
        . "argv=$contract->{argv_text} env=$contract->{environment_text} "
        . "identities=$contract->{identity_text} result=$args{result}";
}

sub run_shared_mode_scheduler {
    my (%args) = @_;
    my @observations;
    for my $mode (@MODES) {
        my $workload = $args{workload}->($mode);
        my $termination = $workload->{termination};
        if ($termination eq 'infrastructure' || $termination eq 'crash') {
            $args{stop}->($mode, $workload) if $args{stop};
            return { observations => \@observations, stopped => 1 };
        }
        my $validation = $args{validator}->($mode, $workload);
        push @observations, {
            mode => $mode, process => $workload, validation => $validation,
        };
    }
    return { observations => \@observations, stopped => 0 };
}

sub run_hardening_schedule_fixture {
    my (%args) = @_;
    my %contract = (
        binary => '/fixture/frozen-libtest',
        binary_identity => 'c' x 64,
        host_identity => 'd' x 64,
        profile_identity => 'e' x 64,
        inventory_identity => 'f' x 64,
    );
    my @lines = (
        'typokat-wu0e-schedule-journal-v1',
        'build count=1 binary=' . ('c' x 64),
    );
    for my $line (@lines) {
        run_append_callback(
            probe => $args{probe}, sink => $args{sink}, line => $line,
            require_file => $args{require_file}, exit_code => 0) == 0
            or fail('schedule journal header callback failed');
    }
    my $seq = 0;
    run_shared_mode_scheduler(
        workload => sub {
            my ($mode) = @_;
            my $stop = defined $args{stop_seq} && $seq == $args{stop_seq};
            my $result = $stop ? 'infrastructure' : 'normal';
            my $line = schedule_callback_line(
                %contract, seq => $seq, kind => 'workload', mode => $mode,
                trace_path => "/fixture/$mode.trace", result => $result);
            my $status = run_append_callback(
                probe => $args{probe}, sink => $args{sink}, line => $line,
                require_file => $args{require_file}, exit_code => $stop ? 73 : 0);
            push @lines, $line;
            $stop ? $status == 73 : $status == 0
                or fail('schedule workload callback status changed');
            ++$seq unless $stop;
            return { termination => $result, sequence => $seq };
        },
        validator => sub {
            my ($mode) = @_;
            my $line = schedule_callback_line(
                %contract, seq => $seq, kind => 'validator', mode => $mode,
                trace_path => "/fixture/$mode.trace", result => 'normal');
            run_append_callback(
                probe => $args{probe}, sink => $args{sink}, line => $line,
                require_file => $args{require_file}, exit_code => 0) == 0
                or fail('schedule validator callback failed');
            push @lines, $line;
            ++$seq;
            return { status => 'complete' };
        },
        stop => sub {
            my (undef, $workload) = @_;
            my $stop_line = "stop after_seq=$workload->{sequence} reason=infrastructure validator_launched=0";
            run_append_callback(
                probe => $args{probe}, sink => $args{sink}, line => $stop_line,
                require_file => $args{require_file}, exit_code => 0) == 0
                or fail('schedule stop journal callback failed');
            push @lines, $stop_line;
        });
    return join("\n", @lines) . "\n";
}

sub capture_simple_child {
    my (%args) = @_;
    my $stdout = create_capture($args{stdout_path});
    my $stderr = create_capture($args{stderr_path});
    my $pid = fork();
    defined $pid or fail("cannot fork bounded evidence child: $!");
    if ($pid == 0) {
        open STDIN, '<', '/dev/null' or die "cannot open /dev/null: $!\n";
        open STDOUT, '>&', $stdout or die "cannot redirect evidence stdout: $!\n";
        open STDERR, '>&', $stderr or die "cannot redirect evidence stderr: $!\n";
        $args{environment}->() if $args{environment};
        exec { $args{command}[0] } @{ $args{command} };
        die "cannot exec evidence child: $!\n";
    }
    close $stdout or fail('cannot close evidence stdout capture');
    close $stderr or fail('cannot close evidence stderr capture');
    waitpid($pid, 0) == $pid or fail('cannot reap bounded evidence child');
    my $status = decode_wait_status($?);
    my (undef, $stdout_oversized) =
        read_bounded_file($args{stdout_path}, $args{limit} // 1024);
    my (undef, $stderr_oversized) =
        read_bounded_file($args{stderr_path}, $args{limit} // 1024);
    !$stdout_oversized && !$stderr_oversized
        or fail('bounded evidence child exceeded capture limit');
    return $status;
}

sub write_canonical_error_artifact {
    my ($path, $expected, $callback) = @_;
    my $ok = eval {
        $callback->();
        1;
    };
    !$ok or fail("expected hardening rejection did not occur: $expected");
    my $error = $@;
    $error =~ s/\n\z//;
    $error =~ s/\Awu0e-diagnostic: //;
    $error eq $expected
        or fail("hardening rejection changed: expected $expected, got $error");
    write_bytes_exclusive($path, "wu0e-diagnostic: $error\n");
}

sub run_marker_rejection_fixture {
    my (%args) = @_;
    my $script = abs_path($0) // fail('cannot resolve marker fixture script');
    my $status = capture_simple_child(
        stdout_path => $args{discard_stdout}, stderr_path => $args{stderr_path},
        limit => 1024, command => ['/usr/bin/perl', $script, '--internal-marker-probe'],
        environment => sub {
            $ENV{$REEXEC_MARKER} = $args{marker};
            $ENV{$REEXEC_PARENT_CGROUP} = self_control_group();
        });
    $status != 0 or fail('marker rejection fixture unexpectedly succeeded');
    unlink $args{discard_stdout}
        or fail('cannot remove marker fixture stdout capture');
    my ($stderr) = read_bounded_file($args{stderr_path}, 1024);
    $stderr eq "wu0e-diagnostic: $args{expected}\n"
        or fail('marker rejection fixture emitted unexpected stderr');
}

sub run_stable_exec_fixture {
    my (%args) = @_;
    my $trusted_sha = sha256_hex(read_regular_input($args{trusted}));
    my ($handle, $identity) = open_stable_executable($args{trusted}, $trusted_sha);
    my $fd = fileno($handle);
    defined $fd or fail('stable fixture handle has no descriptor');
    rename $args{replacement}, $args{trusted}
        or fail("cannot install stable-exec attacker replacement: $!");
    my $stderr_path = File::Spec->catfile($args{fixtures}, 'stable-exec.stderr.tmp');
    my $stdout = create_capture($args{stdout_path});
    my $stderr = create_capture($stderr_path);
    my $pid = fork();
    defined $pid or fail('cannot fork stable-exec fixture');
    if ($pid == 0) {
        open STDOUT, '>&', $stdout or die "cannot redirect stable stdout: $!\n";
        open STDERR, '>&', $stderr or die "cannot redirect stable stderr: $!\n";
        my $fd_path = "/proc/self/fd/$fd";
        exec { $fd_path } $args{trusted};
        die "cannot execute stable fixture handle: $!\n";
    }
    close $stdout or fail('cannot close stable stdout capture');
    close $stderr or fail('cannot close stable stderr capture');
    waitpid($pid, 0) == $pid or fail('cannot reap stable-exec fixture');
    decode_wait_status($?) == 0 or fail('stable-exec fixture failed');
    close $handle or fail('cannot close stable fixture handle');
    my ($stable_stderr) = read_bounded_file($stderr_path, 1024);
    $stable_stderr eq '' or fail('stable-exec fixture emitted stderr');
    unlink $stderr_path or fail('cannot remove stable-exec stderr scratch');
    write_canonical_error_artifact(
        $args{drift_stderr}, 'frozen executable pathname identity drifted',
        sub { verify_stable_executable_path($args{trusted}, $identity) });
}

sub run_candidate_b_validator_drift_fixture {
    my (%args) = @_;
    my @journal = ('typokat-wu0e-candidate-b-validator-path-v1');
    my $validator_index = 0;
    my $schedule = run_shared_mode_scheduler(
        workload => sub { return { termination => 'normal' } },
        validator => sub {
            my ($mode) = @_;
            if ($mode ne 'candidate-b') {
                ++$validator_index;
                return { status => 'complete' };
            }
            $validator_index == 2
                or fail('candidate-b validator was not the final validator');
            push @journal,
                'seq=0 event=scheduler-dispatch kind=validator mode=candidate-b final_validator=1';
            my $contract = diagnostic_launch_contract(
                kind => 'validator', mode => 'candidate-b',
                binary => $args{trusted}, trace_path => $args{trace_path},
                termination => 'normal',
                binary_identity => $args{trusted_identity},
                host_identity => 'd' x 64, profile_identity => 'e' x 64,
                inventory_identity => 'f' x 64);
            my ($result) = hardened_supervise_process(
                scope => $args{scope},
                launch_name => 'fixture-candidate-b-final-validator',
                kind => 'validator', mode => 'candidate-b',
                binary => $args{trusted},
                binary_identity => $args{trusted_identity},
                command => $contract->{command},
                environment => $contract->{environment},
                stdout_path => $args{stdout_path},
                stderr_path => $args{raw_stderr_path},
                meta_path => "$args{raw_stderr_path}.retained.process-meta",
                launch_confirmed_callback => sub {
                    push @journal,
                        'seq=1 event=launch-confirmed membership=1 setsid=1';
                });
            $result->{termination} eq 'normal' && $result->{exit_code} == 0
                or fail('candidate-b validator fixture did not complete');
            push @journal,
                'seq=2 event=trusted-handle-completed exit_code=0';
            my $trusted_original = "$args{trusted}.completed-original";
            rename $args{trusted}, $trusted_original
                or fail("cannot preserve completed validator fixture: $!");
            rename $args{replacement}, $args{trusted}
                or fail("cannot install candidate-b validator replacement: $!");
            push @journal,
                'seq=3 event=pathname-replaced phase=post-completion';
            write_canonical_error_artifact(
                $args{drift_stderr},
                'frozen executable pathname identity drifted',
                sub {
                    verify_completed_frozen_path(
                        $args{trusted}, $args{trusted_identity}, $result);
                });
            push @journal,
                'seq=4 event=path-revalidation outcome=rejected error=frozen-executable-pathname-identity-drifted';
            ++$validator_index;
            return { status => 'complete' };
        },
        stop => sub { fail('candidate-b validator fixture schedule stopped') });
    !$schedule->{stopped} && $validator_index == 3
        or fail('candidate-b validator fixture schedule changed');
    my ($raw_stderr, $raw_stderr_oversized) =
        read_bounded_file($args{raw_stderr_path}, 1024);
    !$raw_stderr_oversized && $raw_stderr eq ''
        or fail('candidate-b trusted validator emitted stderr');
    unlink $args{raw_stderr_path}
        or fail('cannot remove candidate-b validator stderr scratch');
    write_bytes_exclusive(
        $args{journal_path}, join("\n", @journal) . "\n");
}

sub run_artifact_replacement_fixture {
    my (%args) = @_;
    my $victim_before = sha256_hex(read_regular_input($args{victim}));
    my $alias = File::Spec->catfile($args{fixtures}, 'artifact-alias.tmp');
    my $replacement = File::Spec->catfile(
        $args{fixtures}, 'artifact-alias-replacement.tmp');
    link $args{victim}, $alias or fail("cannot create artifact alias: $!");
    write_bytes_exclusive($replacement, "artifact-attacker-v1\n");
    write_canonical_error_artifact(
        $args{stderr_path}, 'artifact inode changed during bounded access', sub {
            read_bounded_file($alias, 1024, sub {
                rename $replacement, $alias
                    or fail("cannot replace artifact alias: $!");
            });
        });
    unlink $alias or fail('cannot remove artifact replacement alias');
    !-e $replacement or fail('artifact replacement temporary survived rename');
    sha256_hex(read_regular_input($args{victim})) eq $victim_before
        or fail('artifact replacement fixture modified its victim');
}

sub run_filesystem_fixtures {
    my (%args) = @_;
    my @lines = ('typokat-wu0e-filesystem-fixtures-v1');
    my $symlink_rejected = !eval {
        assert_real_directory_path($args{symlink_parent}, 'publication parent');
        1;
    };
    $symlink_rejected or fail('symlink publication parent was accepted');
    push @lines, 'case=symlink-parent outcome=rejected error=parent-not-real';

    my $temporary_rejected = !sysopen(
        my $temporary, $args{precreated_temp},
        O_WRONLY | O_CREAT | O_EXCL | O_NOFOLLOW, 0600);
    $temporary_rejected or fail('precreated temporary symlink was accepted');
    push @lines,
        'case=precreated-temp-symlink outcome=rejected error=exclusive-nofollow-create';

    my $publication_temp = File::Spec->catfile(
        $args{fixtures}, 'publication-race-source.tmp');
    write_bytes_exclusive($publication_temp, "publication-race-source-v1\n");
    !link($publication_temp, $args{publication_target})
        or fail('publication target was replaced');
    unlink $publication_temp or fail('cannot remove publication race source');
    push @lines,
        'case=publication-target-race outcome=rejected error=no-replace-publication';
    return join("\n", @lines) . "\n";
}

sub write_key_record {
    my ($path, $prefix, $keys, $fields) = @_;
    my @lines = ($prefix);
    for my $key (@$keys) {
        exists $fields->{$key} or fail("record $prefix lacks $key");
        my $value = $fields->{$key};
        defined $value && "$value" ne '' && "$value" !~ /[\r\n]/
            or fail("record $prefix has invalid $key");
        push @lines, "$key=$value";
    }
    write_bytes_exclusive_fsynced($path, join("\n", @lines) . "\n");
}

sub run_cgroup_preflight_fixture {
    my (%args) = @_;
    my ($result) = hardened_supervise_process(
        scope => $args{scope}, launch_name => 'fixture-preflight',
        kind => 'preflight-fixture', mode => 'fixture',
        binary => $args{perl}, binary_identity => $args{perl_identity},
        command => [$args{perl}, '-e', 'exit 0'], environment => {},
        stdout_path => $args{stdout_path}, stderr_path => $args{stderr_path},
        child_action_path => $args{action_path},
        meta_path => "$args{stderr_path}.retained.process-meta");
    $result->{termination} eq 'normal' && $result->{exit_code} == 0
        && $result->{cgroup_removed}
        or fail('real preflight child did not complete normally');
    my ($child_trace, $child_trace_oversized) =
        read_bounded_file($args{action_path}, 16 * 1024);
    my $expected_child_trace = "typokat-wu0e-preflight-action-trace-v1\n"
        . "actor=child seq=0 action=self-move\n"
        . "actor=child seq=1 action=setsid\n"
        . "actor=child seq=2 action=readiness\n"
        . "actor=child seq=3 action=environment\n"
        . "actor=child seq=4 action=stable-exec\n";
    !$child_trace_oversized && $child_trace eq $expected_child_trace
        or fail('real preflight child action trace changed');
    my @parent_actions = (
        $result->{checked_files} ne ''
            ? 'configure-and-readback outcome=admitted' : (),
        $result->{readiness_seen} ? 'readiness-observed' : (),
        $result->{membership_verified} ? 'membership-verify' : (),
        $result->{setsid_verified} ? 'pgid-verify' : (),
        $result->{termination} eq 'normal' && $result->{cgroup_removed}
            ? 'completion' : (),
    );
    @parent_actions == 5 or fail('real preflight parent action trace is incomplete');
    sysopen my $action_append, $args{action_path}, O_WRONLY | O_APPEND | O_NOFOLLOW
        or fail("cannot append preflight parent action trace: $!");
    for my $sequence (0 .. $#parent_actions) {
        my $line = "actor=parent seq=$sequence action=$parent_actions[$sequence]\n";
        my $written = syswrite($action_append, $line);
        defined $written && $written == length($line)
            or fail("cannot write preflight parent action trace: $!");
    }
    close $action_append or fail('cannot close preflight parent action trace');
    my ($action_trace, $action_trace_oversized) =
        read_bounded_file($args{action_path}, 16 * 1024);
    !$action_trace_oversized or fail('preflight action trace exceeded bound');
    my @child_actions = $child_trace =~ /actor=child seq=[0-9]+ action=([^\n]+)/g;
    @child_actions == 5 or fail('preflight child action inventory changed');
    $child_actions[-1] eq 'stable-exec'
        or fail('preflight child stable exec action is missing');
    $child_actions[-1] = 'exec';
    my $churn = execute_rss_retry_policy(
        attempt_callback => sub {
            return { status => 'retry',
                problem => 'unresolved cgroup membership churn' };
        },
        clock_callback => sub { 0 }, sleep_callback => sub { return });
    my %fields = (
        cgroup_type => $result->{cgroup_type},
        checked_files => $result->{checked_files},
        child_action_order => join(',', @child_actions),
        parent_readiness_evidence => join(',',
            $result->{membership_verified} ? 'membership' : (),
            $result->{setsid_verified} ? 'pgid' : ()),
        cgroup_kill_access => $result->{cgroup_kill_access},
        memory_max_readback => $result->{memory_max},
        memory_swap_max_readback => $result->{memory_swap_max},
        memory_oom_group_readback => $result->{memory_oom_group},
        rss_retry_attempts => $RSS_RETRY_ATTEMPTS,
        rss_retry_deadline_us => $RSS_RETRY_DEADLINE_US,
        unresolved_churn_termination_infrastructure =>
            $churn->{status} eq 'infrastructure' ? 1 : 0,
        launch_cgroup => $result->{launch_cgroup},
        action_trace_source => 'real-hardened-child',
        action_trace_artifact => 'preflight-action.journal',
        action_trace_sha256 => sha256_hex($action_trace),
        action_trace_launch_count => 1,
    );
    write_key_record(
        $args{path}, 'typokat-wu0e-cgroup-preflight-v1',
        [qw(
            cgroup_type checked_files child_action_order
            parent_readiness_evidence cgroup_kill_access memory_max_readback
            memory_swap_max_readback memory_oom_group_readback
            rss_retry_attempts rss_retry_deadline_us
            unresolved_churn_termination_infrastructure launch_cgroup
            action_trace_source action_trace_artifact action_trace_sha256
            action_trace_launch_count
        )], \%fields);
    unlink $args{stdout_path} or fail('cannot remove preflight stdout scratch');
    unlink $args{stderr_path} or fail('cannot remove preflight stderr scratch');
}

sub empty_launch_result {
    my (%args) = @_;
    my $launch = $args{launch};
    my $result = initial_launch_result(%args);
    $result->{cleanup_populated_zero} = 1;
    $result->{cleanup_pgid_empty} = 1;
    capture_final_cgroup_memory($launch, $result);
    return $result;
}

sub run_monitor_exception_fixture {
    my (%args) = @_;
    my ($result) = hardened_supervise_process(
        scope => $args{scope}, launch_name => 'fixture-monitor-exception',
        kind => 'monitor-exception-fixture', mode => 'fixture',
        binary => $args{perl}, binary_identity => $args{perl_identity},
        command => [$args{perl}, '-e',
            '$SIG{TERM}="IGNORE"; select undef,undef,undef,10 while 1'],
        stdout_path => $args{stdout_path}, stderr_path => $args{stderr_path},
        deadline_us => 2_000_000,
        inject_monitor_exception => 'synthetic-monitor-exception');
    $result->{termination} eq 'infrastructure'
        && $result->{infrastructure_error} eq 'synthetic-monitor-exception'
        or fail('monitor exception fixture did not classify as infrastructure');
    write_process_meta_v2($args{meta_path}, $result, {});
    unlink $args{stdout_path} or fail('cannot remove monitor stdout scratch');
    unlink $args{stderr_path} or fail('cannot remove monitor stderr scratch');
}

sub run_retained_exception_fixture {
    my (%args) = @_;
    my @journal = ('typokat-wu0e-retained-exception-order-v1');
    my $next_sequence = 0;
    my $launch_path = File::Spec->catdir(
        $args{scope}{path}, 'fixture-retained-exception');
    my $record_event = sub {
        my ($event, @details) = @_;
        if ($event eq 'outer-exception') {
            $next_sequence == 0 && $details[0] eq
                'synthetic-retained-lifecycle-exception'
                or fail('retained exception outer event changed');
            push @journal,
                'seq=0 event=outer-exception phase=post-fork '
                . 'error=synthetic-retained-lifecycle-exception';
        } elsif ($event eq 'process-meta-fsynced') {
            $next_sequence == 1 && $details[0] eq $args{meta_path}
                && ($details[1]{cgroup_retained} // 0) == 1
                && ($details[1]{meta_fsync_completed} // 0) == 1
                or fail('retained exception process metadata event changed');
            push @journal,
                "seq=1 event=process-meta-fsynced path=$details[0] "
                . 'mandatory_fields=complete cgroup_retained=1';
        } elsif ($event eq 'scope-identity-reverified') {
            $next_sequence == 2
                && $details[0] eq $args{scope}{unit}
                && $details[1] eq $args{scope}{control_group}
                or fail('retained exception scope identity event changed');
            push @journal,
                "seq=2 event=scope-identity-reverified unit=$details[0] "
                . "control_group=$details[1] delegate=yes";
        } elsif ($event eq 'scope-abort-requested') {
            $next_sequence == 3 && ref($details[0]) eq 'ARRAY'
                or fail('retained exception scope abort event changed');
            my $argv = join('|', @{ $details[0] });
            push @journal,
                "seq=3 event=scope-abort-requested argv=$argv";
        } elsif ($event eq 'outer-exception-propagated') {
            $next_sequence == 4 && $details[0] eq
                'synthetic-retained-lifecycle-exception'
                or fail('retained exception propagation event changed');
            push @journal,
                'seq=4 event=outer-exception-propagated '
                . 'error=synthetic-retained-lifecycle-exception';
        } else {
            fail("unknown retained exception event $event");
        }
        ++$next_sequence;
    };
    write_canonical_error_artifact(
        $args{stderr_path}, 'synthetic-retained-lifecycle-exception', sub {
            hardened_supervise_process(
                scope => $args{scope},
                launch_name => 'fixture-retained-exception',
                kind => 'retained-exception-fixture', mode => 'fixture',
                binary => $args{perl}, binary_identity => $args{perl_identity},
                command => [$args{perl}, '-e',
                    'select undef,undef,undef,10 while 1'],
                stdout_path => $args{stdout_scratch},
                stderr_path => $args{stderr_scratch},
                meta_path => $args{meta_path},
                inject_outer_exception_after_fork =>
                    'synthetic-retained-lifecycle-exception',
                retained_event_callback => $record_event,
                scope_abort_request_callback => sub {
                    my ($unit, $control_group, $systemctl_command) = @_;
                    run_scope_abort_spy(
                        spy => $args{spy}, sink => $args{sink},
                        meta_path => $args{meta_path}, unit => $unit,
                        control_group => $control_group,
                        systemctl_command => $systemctl_command);
                    my $events = cgroup_events($launch_path);
                    $events->{populated} == 0
                        or fail('retained exception launch cgroup is populated');
                    rmdir $launch_path
                        or fail('cannot remove retained exception launch cgroup');
                    return {
                        abort_request_observed => 1,
                        retained_launch_removed => !-e $launch_path ? 1 : 0,
                    };
                });
        });
    $next_sequence == 5
        or fail('retained exception event sequence is incomplete');
    !-e $launch_path
        or fail('retained exception launch survived scope abort');
    write_bytes_exclusive(
        $args{journal_path}, join("\n", @journal) . "\n");
    unlink $args{stdout_scratch}
        or fail('cannot remove retained exception stdout scratch');
    unlink $args{stderr_scratch}
        or fail('cannot remove retained exception stderr scratch');
}

sub append_failure_order_line {
    my (%args) = @_;
    my $status = run_append_callback(
        probe => $args{probe}, sink => $args{sink}, line => $args{line},
        require_file => $args{require_file}, exit_code => 0);
    $status == 0 or fail('failure-order callback failed');
    push @{ $args{lines} }, $args{line};
}

sub run_nested_failure_fixture {
    my (%args) = @_;
    my @lines = ('typokat-wu0e-failure-order-v1');
    run_append_callback(
        probe => $args{probe}, sink => $args{sink}, line => $lines[0],
        require_file => $args{require_file}, exit_code => 0) == 0
        or fail('failure-order header callback failed');
    append_failure_order_line(
        %args, lines => \@lines, line => 'seq=0 event=nested-probe-start');
    my ($result, undef, $stderr) = hardened_supervise_process(
        scope => $args{scope}, launch_name => 'fixture-nested-failure',
        kind => 'nested-failure-fixture', mode => 'fixture',
        binary => $args{probe_binary},
        binary_identity => sha256_hex(read_regular_input($args{probe_binary})),
        command => [$args{probe_binary}], stdout_path => $args{stdout_scratch},
        stderr_path => $args{stderr_scratch}, deadline_us => 2_000_000,
        post_infrastructure_error => 'nested-self-test-failure');
    $result->{exit_code} == 73 or fail('nested failure status changed');
    append_failure_order_line(
        %args, lines => \@lines,
        line => 'seq=1 event=nested-probe-exit status=73');
    my $stderr_sha = sha256_hex($stderr);
    append_failure_order_line(
        %args, lines => \@lines,
        line => "seq=2 event=stderr-captured sha256=$stderr_sha");
    write_process_meta_v2(
        $args{meta_path}, $result, { meta_fsync_completed => 1 });
    append_failure_order_line(
        %args, lines => \@lines,
        line => "seq=3 event=process-meta-created path=$args{meta_path}");
    append_failure_order_line(
        %args, lines => \@lines,
        line => "seq=4 event=process-meta-fsynced path=$args{meta_path}");
    write_bytes_exclusive($args{stderr_path}, $stderr);
    append_failure_order_line(
        %args, lines => \@lines,
        line => "seq=5 event=failure-stderr-published sha256=$stderr_sha");
    write_bytes_exclusive($args{status_path}, "73\n");
    append_failure_order_line(
        %args, lines => \@lines,
        line => 'seq=6 event=failure-status-published status=73');
    my $journal = join("\n", @lines) . "\n";
    write_bytes_exclusive($args{journal_path}, $journal);
    unlink $args{stdout_scratch} or fail('cannot remove nested stdout scratch');
    unlink $args{stderr_scratch} or fail('cannot remove nested stderr scratch');
}

sub run_low_memory_fixture {
    my (%args) = @_;
    my $program = <<'PERL';
my @blocks;
while (1) {
    my $block = 'x' x (1024 * 1024);
    substr($block, 0, 1) = 'y';
    push @blocks, $block;
}
PERL
    my ($result) = hardened_supervise_process(
        scope => $args{scope}, launch_name => 'fixture-low-memory',
        kind => 'low-memory-kernel-fixture', mode => 'fixture',
        binary => $args{perl}, binary_identity => $args{perl_identity},
        command => [$args{perl}, '-e', $program],
        stdout_path => $args{stdout_path}, stderr_path => $args{stderr_path},
        memory_max => $SELF_TEST_MEMORY_MAX, rss_limit => $MAX_PROCESS_GROUP_RSS_BYTES,
        deadline_us => 10_000_000);
    $result->{termination} eq 'rss'
        or fail("low-memory fixture terminated as $result->{termination}");
    $result->{events_max_delta} > 0
        && $result->{memory_source} =~ /\A(?:oom|oom_kill|oom_group_kill)\z/
        or fail('low-memory fixture lacks causal kernel memory events');
    write_process_meta_v2(
        $args{meta_path}, $result, { real_kernel_event => 1 });
    unlink $args{stdout_path} or fail('cannot remove low-memory stdout scratch');
    unlink $args{stderr_path} or fail('cannot remove low-memory stderr scratch');
}

sub create_synthetic_drain_fixture {
    my (%args) = @_;
    my $fixture = read_regular_input($args{fixture_path});
    my $expected = "typokat-wu0e-synthetic-drain-view-v1\n"
        . "source=rust-owned-injected-policy-input\n"
        . "cgroup_populated=1\npgid_empty=0\ndrain_expired=1\n";
    $fixture eq $expected or fail('synthetic drain fixture changed');
    my $launch = configure_launch_cgroup(
        scope => $args{scope}, name => 'fixture-synthetic-drain-retained',
        memory_max => $PRODUCTION_MEMORY_MAX);
    my $result = empty_launch_result(
        scope => $args{scope}, launch => $launch,
        kind => 'synthetic-drain-policy-fixture',
        infrastructure_error => 'post-kill-drain-expired');
    @$result{qw(
        direct_kill_attempted pgid_kill_attempted cgroup_kill_attempted
    )} = (1, 1, 1);
    record_emergency_attempt($result, 0);
    record_emergency_attempt($result, 0);
    $result->{cleanup_populated_zero} = 0;
    $result->{cleanup_pgid_empty} = 0;
    $result->{cgroup_retained} = 1;
    my %extra = (
        fixture_source => 'rust-owned-injected-policy-input',
        drain_view_source => 'synthetic-injected',
        fixture_sha256 => sha256_hex($fixture),
        injected_cgroup_populated => 1, injected_pgid_empty => 0,
        injected_drain_expired => 1, scope_abort_requested => 1,
    );
    write_process_meta_v2($args{meta_path}, $result, \%extra);
    return {
        path => $launch->{path},
        retained_at_process_meta => $result->{cgroup_retained},
    };
}

sub create_teardown_failure_fixture {
    my (%args) = @_;
    my $launch = configure_launch_cgroup(
        scope => $args{scope}, name => 'fixture-teardown-failure',
        memory_max => $PRODUCTION_MEMORY_MAX);
    my $result = empty_launch_result(
        scope => $args{scope}, launch => $launch,
        kind => 'delegated-root-teardown-fixture',
        infrastructure_error => 'synthetic-delegated-root-teardown-failure');
    $result->{cleanup_populated_zero} = 1;
    $result->{cleanup_pgid_empty} = 1;
    $result->{cgroup_removed} = 1;
    $result->{cleanup} = 'removed';
    rmdir $launch->{path}
        or fail('cannot remove teardown failure fixture cgroup');
    write_process_meta_v2(
        $args{meta_path}, $result, { scope_abort_requested => 1 });
}

sub assert_fixture_file {
    my ($path, $label, $executable) = @_;
    my @stat = lstat $path;
    @stat && -f _ && !-l _ && (!$executable || -x _)
        or fail("unsafe Rust-owned $label fixture: $path");
}

sub evidence_file {
    my ($directory, $name) = @_;
    $name =~ /\A[A-Za-z0-9_.-]+\z/ or fail('unsafe evidence artifact name');
    return File::Spec->catfile($directory, $name);
}

sub assert_evidence_inventory {
    my ($directory) = @_;
    my @expected = sort qw(
        artifact-replacement.stderr candidate-b-validator-launch.journal
        candidate-b-validator-path-drift.stderr candidate-b-validator.stdout
        delegation.journal delegation.meta
        dossier-equal.sha256 dossier-equal.txt dossier-mismatch.stderr
        filesystem-cases.txt failure-order.journal forged-marker.stderr
        linux-state-cases.txt low-memory.process-meta
        monitor-exception.process-meta nested-failure.process-meta
        nested-failure.status nested-failure.stderr nested-marker.stderr
        preflight-action.journal preflight-failures.txt preflight.meta
        production-hook-routing.journal reexec-argv.txt
        retained-exception-order.journal retained-exception.process-meta
        retained-exception.stderr rss-churn-cases.txt
        schedule-complete.journal schedule-stop.journal
        scope-abort.outcome stable-exec.path-drift.stderr stable-exec.stdout
        synthetic-drain-retention.process-meta systemd-run-count
        teardown-failure.process-meta termination-cases.txt wrapper.status
        wrapper.stderr wrapper.stdout
    );
    opendir my $handle, $directory
        or fail("cannot inspect evidence directory $directory: $!");
    my @actual = sort grep { $_ ne '.' && $_ ne '..' } readdir $handle;
    closedir $handle or fail("cannot close evidence directory $directory: $!");
    join("\0", @actual) eq join("\0", @expected)
        or fail('hardening evidence artifact inventory changed');
}

sub write_delegation_evidence {
    my (%args) = @_;
    my $scope = $args{scope};
    my $runner_controller = $scope->{enabled_by_runner} ? 'memory' : 'none';
    my %fields = (
        scope_unit => $scope->{unit},
        proc_control_group => $scope->{control_group},
        systemctl_control_group => $scope->{control_group},
        systemctl_delegate => $scope->{delegate},
        runner_enabled_controller => $runner_controller,
        controllers_before => $scope->{controllers_before},
        controllers_after => $args{controllers_after},
        supervisor_cgroup => $scope->{supervisor},
        coordinator_pid => $args{coordinator_pid},
        coordinator_start_ticks => $args{coordinator_start_ticks},
        teardown_termination => 'normal',
    );
    write_key_record(
        evidence_file($args{evidence}, 'delegation.meta'),
        'typokat-wu0e-delegation-meta-v1',
        [qw(
            scope_unit proc_control_group systemctl_control_group
            systemctl_delegate runner_enabled_controller controllers_before
            controllers_after supervisor_cgroup coordinator_pid
            coordinator_start_ticks teardown_termination
        )], \%fields);
    my $completion_event = $args{route} eq 'evidence'
        ? 'launch-fixtures-complete' : 'production-schedule-complete';
    my @events = (
        "event=scope-cross-check control_group=$scope->{control_group} delegate=yes",
        "event=supervisor-created path=$scope->{supervisor}",
        "event=coordinator-moved pid=$args{coordinator_pid} destination=$scope->{supervisor}",
        'event=delegated-root-empty members=0',
    );
    push @events, 'event=controller-enabled name=memory'
        if $scope->{enabled_by_runner};
    push @events, "event=$completion_event";
    push @events, 'event=controller-disabled name=memory'
        if $scope->{enabled_by_runner};
    push @events,
        "event=coordinator-moved-back pid=$args{coordinator_pid} destination=$scope->{path}",
        'event=supervisor-empty members=0',
        "event=supervisor-removed path=$scope->{supervisor}";
    my $journal = "typokat-wu0e-delegation-journal-v1\n"
        . join('', map { "seq=$_ $events[$_]\n" } 0 .. $#events);
    write_bytes_exclusive(
        evidence_file($args{evidence}, 'delegation.journal'), $journal);
}

sub hardening_self_test_evidence {
    my (%args) = @_;
    my $evidence = abs_path($args{evidence})
        // fail('cannot resolve hardening evidence directory');
    my $fixtures = abs_path($args{fixtures})
        // fail('cannot resolve hardening fixture directory');
    assert_real_directory_path($evidence, 'hardening evidence directory');
    assert_real_directory_path($fixtures, 'hardening fixture directory');
    $args{nonce} =~ /\A[0-9a-f]{16}\z/ or fail('invalid hardening evidence nonce');
    my $scope = setup_delegated_root($args{scope});
    $scope->{enabled_by_runner} == 1
        or fail('hardening fixture scope inherited an enabled memory controller');
    my $coordinator = hardened_linux_process_stat($$)
        // fail('cannot inspect hardening coordinator identity');

    my %fixture = map {
        $_ => File::Spec->catfile($fixtures, $_)
    } qw(
        trusted-exec.pl replacement-exec.pl victim.bin append-probe.pl
        nested-failure.pl schedule-complete.sink schedule-stop.sink
        failure-order.sink scope-abort-spy.pl scope-abort-spy.sink
        retained-exception-scope-abort-spy.pl
        retained-exception-scope-abort.sink production-hook-probe.pl
        production-hook-routing.sink production-hook-seed.fixture
        candidate-b-validator-trusted.pl candidate-b-validator-replacement.pl
        synthetic-drain-view.fixture symlink-parent precreated-temp.bin
        publication-target.bin
    );
    for my $name (qw(
        trusted-exec.pl replacement-exec.pl append-probe.pl nested-failure.pl
        scope-abort-spy.pl retained-exception-scope-abort-spy.pl
        production-hook-probe.pl candidate-b-validator-trusted.pl
        candidate-b-validator-replacement.pl
    )) {
        assert_fixture_file($fixture{$name}, $name, 1);
    }
    for my $name (qw(
        victim.bin schedule-complete.sink schedule-stop.sink failure-order.sink
        scope-abort-spy.sink retained-exception-scope-abort.sink
        production-hook-routing.sink production-hook-seed.fixture
        synthetic-drain-view.fixture publication-target.bin
    )) {
        assert_fixture_file($fixture{$name}, $name, 0);
    }

    my $dossier = hardening_dossier_fixture($scope);
    write_bytes_exclusive(evidence_file($evidence, 'dossier-equal.txt'), $dossier);
    write_bytes_exclusive(
        evidence_file($evidence, 'dossier-equal.sha256'),
        sha256_hex($dossier) . "\n");
    write_canonical_error_artifact(
        evidence_file($evidence, 'dossier-mismatch.stderr'),
        'completed semantic mismatch: plain=' . ('a' x 64)
            . ' measured-off=' . ('b' x 64),
        sub {
            dossier_v2_bytes(
                binary_identity => 'c' x 64, host_identity => 'd' x 64,
                profile_identity => 'e' x 64, inventory_identity => 'f' x 64,
                observations => [
                    { mode => 'plain', semantic_sha256 => 'a' x 64 },
                    { mode => 'measured-off', semantic_sha256 => 'b' x 64 },
                ]);
        });
    write_bytes_exclusive(
        evidence_file($evidence, 'termination-cases.txt'),
        hardening_termination_fixture_bytes());
    write_bytes_exclusive(
        evidence_file($evidence, 'preflight-failures.txt'),
        hardening_preflight_failure_bytes());
    write_bytes_exclusive(
        evidence_file($evidence, 'rss-churn-cases.txt'),
        hardening_rss_churn_fixture_bytes());
    write_bytes_exclusive(
        evidence_file($evidence, 'linux-state-cases.txt'),
        hardening_linux_state_fixture_bytes());

    my $schedule_complete = run_hardening_schedule_fixture(
        probe => $fixture{'append-probe.pl'},
        sink => $fixture{'schedule-complete.sink'},
        require_file => evidence_file($evidence, 'reexec-argv.txt'));
    my $schedule_stop = run_hardening_schedule_fixture(
        probe => $fixture{'append-probe.pl'}, sink => $fixture{'schedule-stop.sink'},
        require_file => evidence_file($evidence, 'reexec-argv.txt'), stop_seq => 2);
    write_bytes_exclusive(
        evidence_file($evidence, 'schedule-complete.journal'), $schedule_complete);
    write_bytes_exclusive(
        evidence_file($evidence, 'schedule-stop.journal'), $schedule_stop);
    my $hook_binary_identity = sha256_hex(
        read_regular_input($fixture{'candidate-b-validator-trusted.pl'}));
    run_production_hook_routing_fixture(
        scope => $scope, scratch => $fixtures,
        binary => $fixture{'candidate-b-validator-trusted.pl'},
        binary_identity => $hook_binary_identity,
        host_identity => 'd' x 64, profile_identity => 'e' x 64,
        inventory_identity => 'f' x 64,
        probe => $fixture{'production-hook-probe.pl'},
        sink => $fixture{'production-hook-routing.sink'},
        seed => $fixture{'production-hook-seed.fixture'},
        journal_path => evidence_file(
            $evidence, 'production-hook-routing.journal'));
    run_candidate_b_validator_drift_fixture(
        scope => $scope,
        trusted => $fixture{'candidate-b-validator-trusted.pl'},
        replacement => $fixture{'candidate-b-validator-replacement.pl'},
        trusted_identity => $hook_binary_identity,
        trace_path => File::Spec->catfile(
            $fixtures, 'candidate-b-validator.trace'),
        stdout_path => evidence_file(
            $evidence, 'candidate-b-validator.stdout'),
        raw_stderr_path => File::Spec->catfile(
            $fixtures, 'candidate-b-validator.stderr.tmp'),
        drift_stderr => evidence_file(
            $evidence, 'candidate-b-validator-path-drift.stderr'),
        journal_path => evidence_file(
            $evidence, 'candidate-b-validator-launch.journal'));

    my $wrapper_status = capture_simple_child(
        stdout_path => evidence_file($evidence, 'wrapper.stdout'),
        stderr_path => evidence_file($evidence, 'wrapper.stderr'),
        command => ['/usr/bin/perl', '-e',
            'print "wrapper-stdout\\n"; print STDERR "wrapper-stderr\\n"; exit 23'],
        limit => 1024);
    write_bytes_exclusive(
        evidence_file($evidence, 'wrapper.status'), "$wrapper_status\n");
    $wrapper_status == 23 or fail('wrapper passthrough status changed');

    run_marker_rejection_fixture(
        marker => 'forged', expected => 'forged delegated-scope marker',
        discard_stdout => File::Spec->catfile($fixtures, 'forged-marker.stdout.tmp'),
        stderr_path => evidence_file($evidence, 'forged-marker.stderr'));
    run_marker_rejection_fixture(
        marker => $ENV{$REEXEC_MARKER}, expected => 'nested delegated-scope reexec',
        discard_stdout => File::Spec->catfile($fixtures, 'nested-marker.stdout.tmp'),
        stderr_path => evidence_file($evidence, 'nested-marker.stderr'));

    run_stable_exec_fixture(
        fixtures => $fixtures, trusted => $fixture{'trusted-exec.pl'},
        replacement => $fixture{'replacement-exec.pl'},
        stdout_path => evidence_file($evidence, 'stable-exec.stdout'),
        drift_stderr => evidence_file($evidence, 'stable-exec.path-drift.stderr'));
    run_artifact_replacement_fixture(
        fixtures => $fixtures, victim => $fixture{'victim.bin'},
        stderr_path => evidence_file($evidence, 'artifact-replacement.stderr'));
    my $filesystem = run_filesystem_fixtures(
        fixtures => $fixtures, symlink_parent => $fixture{'symlink-parent'},
        precreated_temp => $fixture{'precreated-temp.bin'},
        publication_target => $fixture{'publication-target.bin'});
    write_bytes_exclusive(
        evidence_file($evidence, 'filesystem-cases.txt'), $filesystem);

    my $perl_identity = sha256_hex(read_regular_input('/usr/bin/perl'));
    run_cgroup_preflight_fixture(
        scope => $scope, perl => '/usr/bin/perl',
        perl_identity => $perl_identity,
        path => evidence_file($evidence, 'preflight.meta'),
        action_path => evidence_file($evidence, 'preflight-action.journal'),
        stdout_path => File::Spec->catfile($fixtures, 'preflight.stdout.tmp'),
        stderr_path => File::Spec->catfile($fixtures, 'preflight.stderr.tmp'));
    run_monitor_exception_fixture(
        scope => $scope, perl => '/usr/bin/perl', perl_identity => $perl_identity,
        stdout_path => File::Spec->catfile($fixtures, 'monitor.stdout.tmp'),
        stderr_path => File::Spec->catfile($fixtures, 'monitor.stderr.tmp'),
        meta_path => evidence_file($evidence, 'monitor-exception.process-meta'));
    run_retained_exception_fixture(
        scope => $scope, perl => '/usr/bin/perl', perl_identity => $perl_identity,
        stdout_scratch => File::Spec->catfile(
            $fixtures, 'retained-exception.stdout.tmp'),
        stderr_scratch => File::Spec->catfile(
            $fixtures, 'retained-exception.stderr.tmp'),
        stderr_path => evidence_file($evidence, 'retained-exception.stderr'),
        meta_path => evidence_file($evidence, 'retained-exception.process-meta'),
        journal_path => evidence_file(
            $evidence, 'retained-exception-order.journal'),
        spy => $fixture{'retained-exception-scope-abort-spy.pl'},
        sink => $fixture{'retained-exception-scope-abort.sink'});
    run_nested_failure_fixture(
        scope => $scope, probe_binary => $fixture{'nested-failure.pl'},
        probe => $fixture{'append-probe.pl'}, sink => $fixture{'failure-order.sink'},
        require_file => evidence_file($evidence, 'reexec-argv.txt'),
        stdout_scratch => File::Spec->catfile($fixtures, 'nested.stdout.tmp'),
        stderr_scratch => File::Spec->catfile($fixtures, 'nested.stderr.tmp'),
        stderr_path => evidence_file($evidence, 'nested-failure.stderr'),
        status_path => evidence_file($evidence, 'nested-failure.status'),
        meta_path => evidence_file($evidence, 'nested-failure.process-meta'),
        journal_path => evidence_file($evidence, 'failure-order.journal'));
    run_low_memory_fixture(
        scope => $scope, perl => '/usr/bin/perl', perl_identity => $perl_identity,
        stdout_path => File::Spec->catfile($fixtures, 'low-memory.stdout.tmp'),
        stderr_path => File::Spec->catfile($fixtures, 'low-memory.stderr.tmp'),
        meta_path => evidence_file($evidence, 'low-memory.process-meta'));
    my $synthetic_meta_path = evidence_file(
        $evidence, 'synthetic-drain-retention.process-meta');
    my $synthetic_retained = create_synthetic_drain_fixture(
        scope => $scope, fixture_path => $fixture{'synthetic-drain-view.fixture'},
        meta_path => $synthetic_meta_path);
    create_teardown_failure_fixture(
        scope => $scope,
        meta_path => evidence_file($evidence, 'teardown-failure.process-meta'));

    my $abort_outcome = request_verified_scope_abort(
        scope => $scope,
        injected_request_callback => sub {
            my ($unit, $control_group, $systemctl_command) = @_;
            $unit eq $scope->{unit} && $control_group eq $scope->{control_group}
                or fail('injected scope abort identity mismatch');
            run_scope_abort_spy(
                spy => $fixture{'scope-abort-spy.pl'},
                sink => $fixture{'scope-abort-spy.sink'},
                meta_path => $synthetic_meta_path,
                unit => $unit, control_group => $control_group,
                systemctl_command => $systemctl_command);
            my $events = cgroup_events($synthetic_retained->{path});
            $events->{populated} == 0
                or fail('synthetic retained launch cgroup is still populated');
            rmdir $synthetic_retained->{path}
                or fail('injected scope abort did not remove retained launch cgroup');
            return {
                abort_request_observed => 1,
                retained_launch_removed =>
                    !-e $synthetic_retained->{path} ? 1 : 0,
            };
        });
    $abort_outcome->{abort_request_callback_count} == 1
        or fail('injected scope abort request count changed');
    $abort_outcome->{retained_launch_removed} == 1
        or fail('injected scope abort left the retained launch cgroup present');
    !-e $synthetic_retained->{path}
        or fail('synthetic retained launch cgroup survived scope abort');
    write_bytes_exclusive(
        evidence_file($evidence, 'scope-abort.outcome'),
        join(' ', 'typokat-wu0e-scope-abort-v2',
            "retained_at_process_meta=$synthetic_retained->{retained_at_process_meta}",
            "abort_request_observed=$abort_outcome->{abort_request_observed}",
            "abort_request_callback_count=$abort_outcome->{abort_request_callback_count}",
            "systemctl_argv=$abort_outcome->{systemctl_argv}",
            "retained_launch_removed=$abort_outcome->{retained_launch_removed}",
            'outer_scope_observation=deferred-to-rust-parent') . "\n");
    my $controllers_after = teardown_delegated_root($scope);
    write_delegation_evidence(
        evidence => $evidence, scope => $scope, route => 'evidence',
        controllers_after => $controllers_after, coordinator_pid => $$,
        coordinator_start_ticks => $coordinator->{start_ticks});
    assert_evidence_inventory($evidence);
    print "typokat-wu0e-hardening-evidence-v1 result=ok nonce=$args{nonce} evidence_dir=$evidence\n";
}

sub repo_root {
    my $script = abs_path($0) // fail("cannot resolve script path $0");
    my $root = abs_path(File::Spec->catdir(dirname($script), '..', '..'))
        // fail('cannot resolve repository root');
    -f File::Spec->catfile($root, 'Cargo.toml')
        or fail("repository root has no Cargo.toml: $root");
    return $root;
}

sub require_real_directory {
    my ($path) = @_;
    my @stat = lstat $path;
    @stat && -d _ && !-l _ or fail("path is not a real directory: $path");
}

sub ensure_real_directory_tree {
    my ($root, @components) = @_;
    assert_real_directory_path($root, 'directory-tree root');
    my $cursor = $root;
    for my $component (@components) {
        $component =~ /\A[A-Za-z0-9_.-]+\z/
            && $component ne '.' && $component ne '..'
            or fail('unsafe directory-tree component');
        $cursor = File::Spec->catdir($cursor, $component);
        my @stat = lstat $cursor;
        if (!@stat) {
            mkdir $cursor, 0700 or do {
                my @raced = lstat $cursor;
                @raced && -d _ && !-l _
                    or fail("cannot create real directory $cursor: $!");
            };
        }
        assert_real_directory_path($cursor, 'directory-tree component');
    }
    return $cursor;
}

sub read_regular_input {
    my ($path) = @_;
    my @stat = lstat $path;
    @stat && -f _ && !-l _
        or fail("input is not a regular non-symlink file: $path");
    return read_bytes($path);
}

sub assert_exact_inventory_names {
    my ($actual, $expected, $label) = @_;
    join("\0", sort @$actual) eq join("\0", sort @$expected)
        or fail("profile directory inventory changed: $label");
}

sub canonical_length_framed_sha256 {
    my ($records) = @_;
    my $digest = Digest::SHA->new(256);
    for my $record (@$records) {
        my ($name, $source) = @$record;
        for my $length (length($name), length($source)) {
            my $high = int($length / 4_294_967_296);
            my $low = $length - $high * 4_294_967_296;
            $digest->add(pack('NN', $high, $low));
        }
        $digest->add($name, $source);
    }
    return $digest->hexdigest;
}

sub strict_profile_inventory {
    my ($root) = @_;
    my $profile_root = File::Spec->catdir(
        $root, 'src', 'library', 'typescript-6.0.3');
    require_real_directory($profile_root);
    my $library_root = File::Spec->catdir($profile_root, 'lib');
    require_real_directory($library_root);

    opendir my $root_dir, $profile_root
        or fail("cannot traverse profile root $profile_root: $!");
    my (@root_files, @root_dirs);
    for my $name (grep { $_ ne '.' && $_ ne '..' } readdir $root_dir) {
        my $path = File::Spec->catfile($profile_root, $name);
        my @stat = lstat $path;
        @stat && !-l _ or fail("unsafe profile entry: $path");
        if (-d _) { push @root_dirs, $name }
        elsif (-f _) { push @root_files, $name }
        else { fail("non-regular profile entry: $path") }
    }
    closedir $root_dir or fail("cannot close profile root: $!");
    assert_exact_inventory_names(\@root_dirs, ['lib'], 'root directories');

    opendir my $lib_dir, $library_root
        or fail("cannot traverse profile library $library_root: $!");
    my @actual_library_files;
    for my $name (grep { $_ ne '.' && $_ ne '..' } readdir $lib_dir) {
        my $path = File::Spec->catfile($library_root, $name);
        my @stat = lstat $path;
        @stat && -f _ && !-l _ or fail("unsafe library entry: $path");
        push @actual_library_files, $name;
    }
    closedir $lib_dir or fail("cannot close profile library: $!");

    my @expected_root_files = qw(
        .gitattributes LICENSE.txt README.md THIRD_PARTY_NOTICE.md
        ThirdPartyNoticeText.txt profile.toml
    );
    assert_exact_inventory_names(
        \@root_files, \@expected_root_files, 'root files');
    my $manifest_path = File::Spec->catfile($profile_root, 'profile.toml');
    my $manifest = read_regular_input($manifest_path);
    sha256_hex($manifest) eq $PROFILE_MANIFEST_SHA256
        or fail('profile manifest fingerprint changed');
    $manifest !~ /\r/ && $manifest =~ /\n\z/
        or fail('profile manifest line endings changed');
    $manifest =~ /^file_count = 82$/m
        && $manifest =~ /^source_bytes = 2936611$/m
        && $manifest =~ /^length_framed_sha256 = "\Q$PROFILE_IDENTITY\E"$/m
        or fail('profile manifest contract changed');

    my @sections = split /^\[\[file\]\]\n/m, $manifest;
    shift @sections;
    @sections == 82 or fail('profile manifest must contain 82 files');
    my (@names, @records, @all_files, @inventory_records);
    my $source_bytes = 0;
    for my $ordinal (0 .. $#sections) {
        my ($actual_ordinal) = $sections[$ordinal] =~ /^ordinal = ([0-9]+)$/m;
        my ($name) = $sections[$ordinal] =~ /^name = "([a-z0-9.]+\.d\.ts)"$/m;
        my ($expected_bytes) = $sections[$ordinal] =~ /^bytes = ([0-9]+)$/m;
        my ($expected_sha) = $sections[$ordinal] =~ /^sha256 = "([0-9a-f]{64})"$/m;
        defined $actual_ordinal && $actual_ordinal == $ordinal
            && defined $name && defined $expected_bytes && defined $expected_sha
            or fail("profile manifest record $ordinal changed");
        my $path = File::Spec->catfile($library_root, $name);
        my $bytes = read_regular_input($path);
        length($bytes) == $expected_bytes && sha256_hex($bytes) eq $expected_sha
            or fail("profile source changed: $name");
        push @names, $name;
        push @records, [$name, $bytes];
        push @inventory_records, ["lib/$name", $bytes];
        push @all_files, $path;
        $source_bytes += length($bytes);
    }
    assert_exact_inventory_names(
        \@actual_library_files, \@names, 'library files');
    $source_bytes == 2_936_611
        && canonical_length_framed_sha256(\@records) eq $PROFILE_IDENTITY
        or fail('strict profile identity changed');

    my $warm_bytes = $source_bytes;
    for my $name (@expected_root_files) {
        my $path = File::Spec->catfile($profile_root, $name);
        my $bytes = $name eq 'profile.toml' ? $manifest : read_regular_input($path);
        $warm_bytes += length($bytes);
        push @all_files, $path;
        push @inventory_records, [$name, $bytes];
        if ($name eq '.gitattributes') {
            $bytes eq $PROFILE_GITATTRIBUTES
                or fail('profile attributes contract changed');
        }
    }
    @all_files == 88 or fail('warm inventory must contain 88 regular files');
    return {
        profile_root => $profile_root,
        source_count => scalar(@names),
        regular_files => scalar(@all_files),
        warm_bytes => $warm_bytes,
        inventory_identity => canonical_length_framed_sha256(\@inventory_records),
    };
}

sub assert_expected_binary {
    my ($path, $expected_sha) = @_;
    my @before = lstat $path;
    @before && -f _ && -x _ && !-l _
        or fail("frozen libtest is unsafe: $path");
    sysopen my $handle, $path, O_RDONLY | O_NOFOLLOW
        or fail("cannot open frozen libtest $path: $!");
    binmode $handle, ':raw';
    my @opened = stat $handle;
    @opened && -f _ && -x _
        or fail("opened frozen libtest is unsafe: $path");
    $before[0] == $opened[0] && $before[1] == $opened[1]
        or fail('frozen libtest pathname identity drifted');
    my $digest = digest_open_handle($handle);
    close $handle or fail("cannot close frozen libtest $path: $!");
    my @after = lstat $path;
    @after && $opened[0] == $after[0] && $opened[1] == $after[1]
        && $opened[7] == $after[7]
        or fail('frozen libtest pathname identity drifted');
    $digest eq $expected_sha
        or fail("frozen libtest digest mismatch: $path");
}

sub validate_and_warm_runtime_inputs {
    my ($root, $binary, $binary_identity, $expected_inventory_identity) = @_;
    assert_expected_binary($binary, $binary_identity);
    my $inventory = strict_profile_inventory($root);
    if (defined $expected_inventory_identity) {
        $inventory->{inventory_identity} eq $expected_inventory_identity
            or fail('strict profile warm-inventory identity changed between launches');
    }
    assert_expected_binary($binary, $binary_identity);
    return $inventory;
}

sub create_run_directory {
    my ($root, $kind) = @_;
    my $base = ensure_real_directory_tree(
        $root, 'target', 'wu0e-diagnostic', $kind);
    my $stamp = strftime('%Y%m%dT%H%M%SZ', gmtime());
    my $serial = 0;
    my $path;
    while (1) {
        my $suffix = $serial == 0 ? '' : "-$serial";
        $path = File::Spec->catdir($base, $stamp . "-$$" . $suffix);
        last if mkdir $path, 0700;
        -e $path or fail("cannot create run directory $path: $!");
        ++$serial;
        $serial <= 1_000 or fail('cannot allocate unique run directory');
    }
    return abs_path($path) // fail("cannot resolve run directory $path");
}

sub run_captured {
    my ($stdout_path, $stderr_path, @command) = @_;
    my $stdout = create_capture($stdout_path);
    my $stderr = create_capture($stderr_path);
    my $pid = fork();
    defined $pid or fail("fork failed for @command: $!");
    if ($pid == 0) {
        open STDOUT, '>&', $stdout or die "cannot redirect stdout: $!\n";
        open STDERR, '>&', $stderr or die "cannot redirect stderr: $!\n";
        exec { $command[0] } @command;
        die "cannot exec $command[0]: $!\n";
    }
    close $stdout or fail("cannot close parent stdout capture: $!");
    close $stderr or fail("cannot close parent stderr capture: $!");
    waitpid($pid, 0) == $pid or fail("waitpid failed for @command: $!");
    return decode_wait_status($?);
}

sub build_release_libtest_once {
    my ($root, $run_dir) = @_;
    my @command = ('cargo', 'test', '--release', '--lib', '--no-run',
        '--message-format=json-render-diagnostics');
    write_bytes_exclusive(File::Spec->catfile($run_dir, 'build-command.txt'),
        command_text(@command) . "\n");
    my $stdout_path = File::Spec->catfile($run_dir, 'cargo-build.jsonl');
    my $stderr_path = File::Spec->catfile($run_dir, 'cargo-build.stderr');
    my $exit = run_captured($stdout_path, $stderr_path, @command);
    $exit == 0 or fail("release libtest build failed; artifacts: $run_dir");
    my @executables;
    for my $line (split /\n/, read_bytes($stdout_path)) {
        next if $line eq '';
        my $message = eval { decode_json($line) };
        defined $message or fail('cargo emitted non-JSON stdout');
        next unless ($message->{reason} // '') eq 'compiler-artifact';
        next unless ref($message->{target}) eq 'HASH';
        next unless ($message->{target}{name} // '') eq 'typokat';
        next unless grep { $_ eq 'lib' } @{ $message->{target}{kind} // [] };
        next unless ref($message->{profile}) eq 'HASH' && $message->{profile}{test};
        push @executables, $message->{executable} if defined $message->{executable};
    }
    @executables == 1 or fail('cargo did not identify exactly one release libtest');
    my $built = abs_path($executables[0])
        // fail("built libtest is missing: $executables[0]");
    return $built;
}

sub freeze_libtest {
    my ($root, $built) = @_;
    my @built_before = lstat $built;
    @built_before && -f _ && -x _ && !-l _
        or fail("built libtest is unsafe: $built");
    sysopen my $source, $built, O_RDONLY | O_NOFOLLOW
        or fail("cannot open built libtest $built: $!");
    binmode $source, ':raw';
    my @built_opened = stat $source;
    @built_opened && $built_before[0] == $built_opened[0]
        && $built_before[1] == $built_opened[1]
        or fail('built libtest identity drifted before freeze');
    my $digest = digest_open_handle($source);
    my $directory = ensure_real_directory_tree(
        $root, 'target', 'wu0e-diagnostic', 'frozen');
    my $frozen = File::Spec->catfile($directory, "typokat-libtest-$digest");
    if (-e $frozen) {
        assert_expected_binary($frozen, $digest);
    } else {
        my $temporary = "$frozen.tmp-$$";
        sysopen my $target,
            $temporary, O_WRONLY | O_CREAT | O_EXCL | O_NOFOLLOW, 0500
            or fail("cannot create frozen libtest temporary: $!");
        binmode $target, ':raw';
        seek($source, 0, 0) or fail("cannot rewind built libtest: $!");
        my $buffer;
        while (1) {
            my $count = sysread($source, $buffer, 128 * 1024);
            defined $count or fail("cannot read built libtest: $!");
            last if $count == 0;
            my $offset = 0;
            while ($offset < $count) {
                my $written = syswrite($target, $buffer, $count - $offset, $offset);
                defined $written && $written > 0
                    or fail("cannot write frozen libtest temporary: $!");
                $offset += $written;
            }
        }
        $target->flush() or fail("cannot flush frozen libtest temporary: $!");
        $target->sync() or fail("cannot fsync frozen libtest temporary: $!");
        close $target or fail("cannot close frozen libtest temporary: $!");
        chmod 0500, $temporary or fail("cannot lock frozen libtest: $!");
        assert_expected_binary($temporary, $digest);
        if (!link $temporary, $frozen) {
            -e $frozen or fail("cannot publish frozen libtest without replacement: $!");
            assert_expected_binary($frozen, $digest);
        }
        unlink $temporary or fail("cannot remove frozen libtest temporary: $!");
    }
    close $source or fail("cannot close built libtest $built: $!");
    my @built_after = lstat $built;
    @built_after && $built_opened[0] == $built_after[0]
        && $built_opened[1] == $built_after[1]
        && $built_opened[7] == $built_after[7]
        or fail('built libtest identity drifted during freeze');
    chmod 0500, $frozen or fail("cannot lock frozen libtest: $!");
    assert_expected_binary($frozen, $digest);
    return ($frozen, $digest);
}

sub capture_command_line {
    my (@command) = @_;
    pipe(my $reader, my $writer) or fail("pipe failed: $!");
    my $pid = fork();
    defined $pid or fail("fork failed: $!");
    if ($pid == 0) {
        close $reader;
        open STDOUT, '>&', $writer or die "redirect failed: $!\n";
        open STDERR, '>&', $writer or die "redirect failed: $!\n";
        exec { $command[0] } @command;
        die "exec failed: $!\n";
    }
    close $writer;
    local $/;
    my $output = <$reader> // '';
    close $reader;
    waitpid($pid, 0) == $pid or fail("waitpid failed: $!");
    decode_wait_status($?) == 0 or fail("command failed: @command");
    $output =~ s/\s+\z//;
    return $output;
}

sub host_facts {
    my @uname = POSIX::uname();
    @uname == 5 or fail('POSIX::uname returned unexpected fields');
    my $machine_id = read_regular_input('/etc/machine-id');
    my $boot_id = read_regular_input('/proc/sys/kernel/random/boot_id');
    $machine_id =~ s/\n\z//;
    $boot_id =~ s/\n\z//;
    my $cpu = -f '/proc/cpuinfo' ? read_regular_input('/proc/cpuinfo') : '';
    my $facts = join "\n",
        'typokat-wu0e-host-v1',
        "sysname=$uname[0]", "nodename=$uname[1]", "release=$uname[2]",
        "version=$uname[3]", "machine=$uname[4]",
        "machine_id=$machine_id", "boot_id=$boot_id",
        'logical_cpus=' . scalar(grep { /^processor\s*:/ } split /\n/, $cpu),
        'rustc=' . capture_command_line('rustc', '-Vv'),
        'cargo=' . capture_command_line('cargo', '-V'), '';
    return ($facts, sha256_hex($facts));
}

sub linux_process_stat {
    my ($pid) = @_;
    my $path = "/proc/$pid/stat";
    sysopen my $handle, $path, O_RDONLY or return;
    my $bytes = '';
    my $count = sysread($handle, $bytes, 4097);
    close $handle;
    return unless defined $count && $count > 0 && $count <= 4096;
    my ($actual_pid, $state, $pgrp) =
        $bytes =~ /\A([0-9]+) \(.*\) ([A-Z]) [0-9]+ ([0-9]+) /;
    return unless defined $actual_pid && $actual_pid == $pid;
    return { state => $state, pgrp => 0 + $pgrp };
}

sub checked_add {
    my ($left, $right) = @_;
    return if $left < 0 || $right < 0 || $left > $MAX_SAFE_INTEGER - $right;
    return $left + $right;
}

sub checked_mul {
    my ($left, $right) = @_;
    return if $left < 0 || $right < 0;
    return 0 if $left == 0 || $right == 0;
    return if $left > int($MAX_SAFE_INTEGER / $right);
    return $left * $right;
}

sub process_rss_bytes {
    my ($pid, $pgrp) = @_;
    my $path = "/proc/$pid/statm";
    sysopen my $handle, $path, O_RDONLY or do {
        my $again = linux_process_stat($pid);
        return (0, undef) unless defined $again
            && $again->{pgrp} == $pgrp
            && $again->{state} ne 'Z' && $again->{state} ne 'X';
        return (undef, "missing live-member RSS for pid $pid");
    };
    my $bytes = '';
    my $count = sysread($handle, $bytes, 257);
    close $handle;
    defined $count && $count > 0 && $count <= 256
        or return (undef, "unreadable live-member RSS for pid $pid");
    my ($resident_pages) = $bytes =~ /\A[0-9]+ ([0-9]+)(?: [0-9]+)*\n?\z/;
    defined $resident_pages
        or return (undef, "malformed live-member RSS for pid $pid");
    my $rss = checked_mul(0 + $resident_pages, $PAGE_SIZE);
    defined $rss or return (undef, "RSS arithmetic overflow for pid $pid");
    return ($rss, undef);
}

sub sample_process_group_rss {
    my ($pgrp) = @_;
    opendir my $proc, '/proc' or return (undef, undef, "cannot inspect /proc: $!");
    my @pids = grep { /\A[0-9]+\z/ } readdir $proc;
    closedir $proc or return (undef, undef, "cannot close /proc: $!");
    my ($sum, $largest) = (0, 0);
    for my $pid (@pids) {
        my $stat = linux_process_stat($pid) // next;
        next unless $stat->{pgrp} == $pgrp
            && $stat->{state} ne 'Z' && $stat->{state} ne 'X';
        my ($rss, $error) = process_rss_bytes(0 + $pid, $pgrp);
        return (undef, undef, $error) if defined $error;
        next unless defined $rss;
        $sum = checked_add($sum, $rss);
        return (undef, undef, 'RSS group sum overflow') unless defined $sum;
        $largest = $rss if $rss > $largest;
    }
    return ($sum, $largest, undef, scalar(grep {
        my $stat = linux_process_stat($_);
        defined $stat && $stat->{pgrp} == $pgrp
            && $stat->{state} ne 'Z' && $stat->{state} ne 'X'
    } @pids));
}

sub group_has_live_members {
    my ($pgrp) = @_;
    my ($sum, $largest, $error, $members) = sample_process_group_rss($pgrp);
    return (undef, $error) if defined $error;
    return ($members > 0, undef);
}

sub scrub_wu_environment {
    my ($allowlist) = @_;
    my $removed = 0;
    for my $key (keys %ENV) {
        if ($key =~ /\ATYPOKAT_WU0[B-E]_/) {
            delete $ENV{$key};
            ++$removed;
        }
    }
    for my $key (keys %$allowlist) {
        $ENV{$key} = $allowlist->{$key};
    }
    return $removed;
}

sub supervise_process {
    my (%args) = @_;
    my $deadline_us = $args{deadline_us} // $DEADLINE_US;
    my $rss_limit = $args{rss_limit} // $MAX_PROCESS_GROUP_RSS_BYTES;
    my $stdout_limit = $args{stdout_limit} // $MAX_STDOUT_BYTES;
    my $stderr_limit = $args{stderr_limit} // $MAX_STDERR_BYTES;
    my $trace_limit = $args{trace_limit} // $MAX_TRACE_BYTES;
    my $term_grace_us = $args{term_grace_us} // $TERM_GRACE_US;
    my $drain_grace_us = $args{drain_grace_us} // $DRAIN_GRACE_US;
    my @command = @{ $args{command} // [] };
    @command or fail('supervisor received an empty command');
    assert_expected_binary($args{binary}, $args{binary_identity});

    my $stdout_handle = create_capture($args{stdout_path});
    my $stderr_handle = create_capture($args{stderr_path});
    pipe(my $ready_reader, my $ready_writer) or fail("readiness pipe failed: $!");
    fcntl($ready_writer, F_SETFD,
        fcntl($ready_writer, F_GETFD, 0) | FD_CLOEXEC)
        or fail("cannot set readiness close-on-exec: $!");
    fcntl($ready_reader, F_SETFL,
        fcntl($ready_reader, F_GETFL, 0) | O_NONBLOCK)
        or fail("cannot make readiness pipe nonblocking: $!");

    my $started = clock_gettime(CLOCK_MONOTONIC);
    my $pid = fork();
    defined $pid or fail("fork failed for supervised process: $!");
    if ($pid == 0) {
        close $ready_reader;
        open STDOUT, '>&', $stdout_handle or die "cannot redirect stdout: $!\n";
        open STDERR, '>&', $stderr_handle or die "cannot redirect stderr: $!\n";
        $args{pre_setsid_setup}->() if $args{pre_setsid_setup};
        if ($args{pre_setsid_delay_us}) {
            usleep($args{pre_setsid_delay_us});
        }
        setsid() >= 0 or die "setsid failed: $!\n";
        syswrite($ready_writer, 'R') == 1
            or die "cannot publish process-group readiness: $!\n";
        close $ready_writer or die "cannot close readiness pipe: $!\n";
        scrub_wu_environment($args{environment} // {});
        $args{child_setup}->() if $args{child_setup};
        exec { $command[0] } @command;
        die "cannot exec $command[0]: $!\n";
    }
    close $stdout_handle or fail("cannot close parent stdout capture: $!");
    close $stderr_handle or fail("cannot close parent stderr capture: $!");
    close $ready_writer;

    my %observed = (
        group_ready => 0, deadline_hit => 0, rss_hit => 0,
        stdout_hit => 0, stderr_hit => 0, trace_hit => 0,
        infrastructure_hit => 0, term_sent => 0, kill_sent => 0,
        direct_kill_attempted => 0, group_kill_attempted => 0,
        drain_expired => 0, zombie_leader_observed => 0,
        max_group_rss_bytes => 0, largest_member_rss_bytes => 0,
        max_rss_sample_interval_us => 0,
    );
    my ($readiness_closed, $term_at, $drain_deadline, $wait_status);
    my $last_sample_at;
    while (1) {
        unless ($observed{group_ready}) {
            my $byte = '';
            my $count = sysread($ready_reader, $byte, 1);
            if (defined $count && $count == 1) {
                $byte eq 'R' or fail('invalid readiness byte');
                $observed{group_ready} = 1;
            } elsif (defined $count && $count == 0) {
                $readiness_closed = 1;
            } elsif (!defined $count && $! != EAGAIN && $! != EWOULDBLOCK) {
                fail("cannot read process readiness: $!");
            }
        }

        my $leader = linux_process_stat($pid);
        defined $leader or fail("leader disappeared before safe reap: $pid");
        my $leader_zombie = $leader->{state} eq 'Z' || $leader->{state} eq 'X';
        $observed{zombie_leader_observed} = 1 if $leader_zombie;
        if ($observed{group_ready} && $leader->{pgrp} != $pid) {
            $observed{infrastructure_hit} = 1;
            $observed{infrastructure_error} = 'leader escaped confirmed process group';
        }

        my $now = clock_gettime(CLOCK_MONOTONIC);
        if ($observed{group_ready}) {
            if (defined $last_sample_at) {
                my $interval = int(($now - $last_sample_at) * 1_000_000 + 0.999999);
                $observed{max_rss_sample_interval_us} = $interval
                    if $interval > $observed{max_rss_sample_interval_us};
            }
            $last_sample_at = $now;
            my ($sum, $largest, $rss_error) = $args{rss_sampler}
                ? $args{rss_sampler}->($pid) : sample_process_group_rss($pid);
            if (defined $rss_error) {
                $observed{infrastructure_hit} = 1;
                $observed{infrastructure_error} = $rss_error;
            } else {
                $observed{max_group_rss_bytes} = $sum
                    if $sum > $observed{max_group_rss_bytes};
                $observed{largest_member_rss_bytes} = $largest
                    if $largest > $observed{largest_member_rss_bytes};
                $observed{rss_hit} = 1 if $sum > $rss_limit;
            }
        }

        my $stdout_bytes = file_size($args{stdout_path}, 0);
        my $stderr_bytes = file_size($args{stderr_path}, 0);
        my $trace_bytes = defined $args{trace_path}
            ? file_size($args{trace_path}, 1) : 0;
        $observed{stdout_hit} = 1 if $stdout_bytes > $stdout_limit;
        $observed{stderr_hit} = 1 if $stderr_bytes > $stderr_limit;
        $observed{trace_hit} = 1 if $trace_bytes > $trace_limit;
        $observed{deadline_hit} = 1
            if ($now - $started) * 1_000_000 >= $deadline_us;

        my $must_terminate = $observed{deadline_hit} || $observed{rss_hit}
            || $observed{stdout_hit} || $observed{stderr_hit}
            || $observed{trace_hit} || $observed{infrastructure_hit};
        if (!$observed{term_sent} && $must_terminate) {
            kill 'TERM', $pid unless $leader_zombie;
            kill 'TERM', -$pid if $observed{group_ready};
            $observed{term_sent} = 1;
            $term_at = $now;
        }
        if ($observed{term_sent} && !$observed{kill_sent}
            && ($now - $term_at) * 1_000_000 >= $term_grace_us) {
            if (!$leader_zombie) {
                kill 'KILL', $pid;
                $observed{direct_kill_attempted} = 1;
            }
            if ($observed{group_ready}) {
                kill 'KILL', -$pid;
                $observed{group_kill_attempted} = 1;
            }
            $observed{kill_sent} = 1;
            $drain_deadline = $now + $drain_grace_us / 1_000_000;
        }

        my ($live_group, $group_error) = (0, undef);
        if ($observed{group_ready} && $leader_zombie) {
            ($live_group, $group_error) = group_has_live_members($pid);
            if (defined $group_error) {
                $observed{infrastructure_hit} = 1;
                $observed{infrastructure_error} = $group_error;
                $live_group = 1;
            }
        }
        my $tree_quiescent = $leader_zombie
            && ($observed{group_ready} ? !$live_group : $readiness_closed);
        if ($tree_quiescent && (!$observed{term_sent} || $observed{kill_sent})) {
            waitpid($pid, WNOHANG) == $pid
                or fail("safe zombie reap failed: $!");
            $wait_status = $?;
            last;
        }
        if ($observed{kill_sent} && $now >= $drain_deadline) {
            $observed{drain_expired} = 1;
            $observed{infrastructure_hit} = 1;
            last;
        }
        usleep($RSS_SAMPLE_TARGET_US);
    }
    my $finished = clock_gettime(CLOCK_MONOTONIC);
    close $ready_reader or fail("cannot close readiness pipe: $!");
    assert_expected_binary($args{binary}, $args{binary_identity});

    $observed{stdout_bytes} = file_size($args{stdout_path}, 0);
    $observed{stderr_bytes} = file_size($args{stderr_path}, 0);
    $observed{trace_bytes} = defined $args{trace_path}
        ? file_size($args{trace_path}, 1) : 0;
    $observed{stdout_hit} ||= $observed{stdout_bytes} > $stdout_limit;
    $observed{stderr_hit} ||= $observed{stderr_bytes} > $stderr_limit;
    $observed{trace_hit} ||= $observed{trace_bytes} > $trace_limit;
    $observed{wait_status} = $wait_status;
    $observed{exit_code} = defined $wait_status
        ? decode_wait_status($wait_status) : 255;
    $observed{elapsed_us} = int(($finished - $started) * 1_000_000 + 0.999999);
    $observed{termination} = $observed{infrastructure_hit} ? 'infrastructure'
        : $observed{trace_hit} ? 'trace'
        : $observed{stdout_hit} ? 'stdout'
        : $observed{stderr_hit} ? 'stderr'
        : $observed{rss_hit} ? 'rss'
        : $observed{deadline_hit} ? 'deadline'
        : $observed{exit_code} == 0 ? 'normal' : 'crash';
    return \%observed;
}

sub workload_command {
    my ($binary) = @_;
    return ($binary, '--ignored', '--exact', $WORKLOAD_PROBE, '--nocapture');
}

sub validator_command {
    my ($binary) = @_;
    return ($binary, '--ignored', '--exact', $VALIDATOR_PROBE, '--nocapture');
}

sub workload_environment {
    my ($mode, $trace_path) = @_;
    return {
        TYPOKAT_WU0E_MODE => $mode,
        TYPOKAT_WU0E_TRACE_PATH => $trace_path,
    };
}

sub validator_environment {
    my ($mode, $trace_path, $termination) = @_;
    return {
        TYPOKAT_WU0E_VALIDATE_TRACE_PATH => $trace_path,
        TYPOKAT_WU0E_VALIDATE_MODE => $mode,
        TYPOKAT_WU0E_VALIDATE_TERMINATION => $termination,
    };
}

sub diagnostic_launch_contract {
    my (%args) = @_;
    scalar(grep { $_ eq $args{mode} } @MODES) == 1
        or fail("unknown diagnostic mode $args{mode}");
    $args{kind} =~ /\A(?:workload|validator)\z/
        or fail('unknown diagnostic launch kind');
    for my $identity (qw(
        binary_identity host_identity profile_identity inventory_identity
    )) {
        defined $args{$identity} && $args{$identity} =~ /\A[0-9a-f]{64}\z/
            or fail("invalid diagnostic launch $identity");
    }
    my @command = $args{kind} eq 'workload'
        ? workload_command($args{binary}) : validator_command($args{binary});
    my $environment = $args{kind} eq 'workload'
        ? workload_environment($args{mode}, $args{trace_path})
        : validator_environment(
            $args{mode}, $args{trace_path}, $args{termination});
    my $environment_text = join('|', map {
        "$_=$environment->{$_}"
    } sort keys %$environment);
    return {
        command => \@command,
        environment => $environment,
        argv_text => join('|', @command),
        environment_text => $environment_text,
        identity_text => join('|', @args{qw(
            binary_identity host_identity profile_identity inventory_identity
        )}),
    };
}

sub verify_completed_frozen_path {
    my ($binary, $binary_identity, $result) = @_;
    verify_stable_executable_path($binary, $result->{_executable_identity});
    assert_expected_binary($binary, $binary_identity);
}

sub write_hardened_result_meta {
    my ($result, $validator_launched) = @_;
    my %public = map { $_ => $result->{$_} }
        grep { $_ !~ /\A_/ } keys %$result;
    $public{validator_launched} = $validator_launched;
    write_process_meta_v2($result->{_meta_path}, \%public, {});
}

sub run_hardened_workload {
    my (%args) = @_;
    my $mode = $args{mode};
    scalar(grep { $_ eq $mode } @MODES) == 1
        or fail("unknown diagnostic mode $mode");
    my $prefix = File::Spec->catfile($args{run_dir}, "workload-$mode");
    my $trace_path = "$prefix.trace";
    my $meta_path = "$prefix.process-meta";
    -e $trace_path and fail("trace path already exists: $trace_path");
    my $contract = diagnostic_launch_contract(
        kind => 'workload', mode => $mode, binary => $args{binary},
        trace_path => $trace_path, termination => 'normal',
        binary_identity => $args{binary_identity},
        host_identity => $args{host_identity},
        profile_identity => $PROFILE_IDENTITY,
        inventory_identity => $args{inventory_identity});
    write_bytes_exclusive(
        "$prefix.command", command_text(@{ $contract->{command} }) . "\n");
    my $warm = validate_and_warm_runtime_inputs(
        $args{root}, $args{binary}, $args{binary_identity},
        $args{inventory_identity});
    my ($result) = hardened_supervise_process(
        scope => $args{scope}, launch_name => "workload-$mode",
        kind => 'workload', mode => $mode,
        binary => $args{binary}, binary_identity => $args{binary_identity},
        command => $contract->{command}, stdout_path => "$prefix.stdout",
        stderr_path => "$prefix.stderr", trace_path => $trace_path,
        environment => $contract->{environment}, meta_path => $meta_path,
        scope_abort_requested_callback =>
            $args{scope_abort_requested_callback});
    $result->{_meta_path} = $meta_path;
    $result->{_trace_path} = $trace_path;
    $result->{warm_regular_files} = $warm->{regular_files};
    $result->{warm_bytes} = $warm->{warm_bytes};
    $result->{binary_identity} = $args{binary_identity};
    $result->{profile_identity} = $PROFILE_IDENTITY;
    $result->{inventory_identity} = $args{inventory_identity};
    verify_completed_frozen_path(
        $args{binary}, $args{binary_identity}, $result);
    return $result;
}

sub run_hardened_validator {
    my (%args) = @_;
    my $mode = $args{mode};
    my $prefix = File::Spec->catfile($args{run_dir}, "validator-$mode");
    my $meta_path = "$prefix.process-meta";
    my $contract = diagnostic_launch_contract(
        kind => 'validator', mode => $mode, binary => $args{binary},
        trace_path => $args{trace_path}, termination => $args{termination},
        binary_identity => $args{binary_identity},
        host_identity => $args{host_identity},
        profile_identity => $PROFILE_IDENTITY,
        inventory_identity => $args{inventory_identity});
    write_bytes_exclusive(
        "$prefix.command", command_text(@{ $contract->{command} }) . "\n");
    my $warm = validate_and_warm_runtime_inputs(
        $args{root}, $args{binary}, $args{binary_identity},
        $args{inventory_identity});
    my ($result, $stdout) = hardened_supervise_process(
        scope => $args{scope}, launch_name => "validator-$mode",
        kind => 'validator', mode => $mode,
        binary => $args{binary}, binary_identity => $args{binary_identity},
        command => $contract->{command}, stdout_path => "$prefix.stdout",
        stderr_path => "$prefix.stderr",
        environment => $contract->{environment}, meta_path => $meta_path,
        launch_confirmed_callback => $args{launch_confirmed_callback},
        scope_abort_requested_callback =>
            $args{scope_abort_requested_callback});
    $result->{_meta_path} = $meta_path;
    $result->{warm_regular_files} = $warm->{regular_files};
    $result->{warm_bytes} = $warm->{warm_bytes};
    $result->{binary_identity} = $args{binary_identity};
    $result->{profile_identity} = $PROFILE_IDENTITY;
    $result->{inventory_identity} = $args{inventory_identity};
    $args{post_completion_callback}->($result)
        if defined $args{post_completion_callback};
    verify_completed_frozen_path(
        $args{binary}, $args{binary_identity}, $result);
    write_hardened_result_meta($result, 0);
    $result->{termination} eq 'normal' && $result->{exit_code} == 0
        or fail("same-binary validator failed; artifacts: $args{run_dir}");
    my @lines = grep { /^typokat-wu0e-validation-v1 / } split /\n/, $stdout;
    @lines == 1
        or fail("validator emitted no unique result; artifacts: $args{run_dir}");
    my ($actual_mode, $termination, $status, $semantic) = $lines[0] =~
        /\Atypokat-wu0e-validation-v1 mode=(plain|measured-off|candidate-b) termination=(normal|deadline|rss|stdout|stderr|trace|crash|infrastructure) status=(complete|partial) semantic_sha256=([0-9a-f]{64}|unavailable)\z/;
    defined $actual_mode && $actual_mode eq $mode
        && $termination eq $args{termination}
        or fail("validator result identity mismatch; artifacts: $args{run_dir}");
    if ($status eq 'complete') {
        $args{termination} eq 'normal' && $semantic ne 'unavailable'
            or fail('validator promoted a contained workload to complete');
    } else {
        $args{termination} ne 'normal' && $semantic eq 'unavailable'
            or fail('validator inferred a digest for a partial workload');
    }
    return { status => $status, semantic => $semantic, line => $lines[0] };
}

sub production_dossier_observation {
    my ($mode, $process, $validation) = @_;
    my %observation = (
        mode => $mode, termination => $process->{termination},
        semantic_sha256 => $validation->{semantic},
        readiness => $process->{readiness_seen},
        membership => $process->{membership_verified},
        setsid => $process->{setsid_verified},
    );
    for my $key (qw(
        scope_unit scope_control_group launch_cgroup memory_max memory_swap_max
        memory_oom_group rss_peak memory_current memory_peak events_max_baseline
        events_max_final events_max_delta events_oom_baseline events_oom_final
        events_oom_delta events_oom_kill_baseline events_oom_kill_final
        events_oom_kill_delta events_oom_group_kill_baseline
        events_oom_group_kill_final events_oom_group_kill_delta memory_source
        direct_kill_attempted pgid_kill_attempted cgroup_kill_attempted
        cleanup_populated_zero cleanup_pgid_empty leader_reaped cgroup_removed
        cgroup_retained cleanup
    )) {
        $observation{$key} = $process->{$key};
    }
    return \%observation;
}

sub live_non_zombie_process {
    my ($pid) = @_;
    my $stat = linux_process_stat($pid);
    return defined $stat && $stat->{state} ne 'Z' && $stat->{state} ne 'X';
}

sub self_test_supervise {
    my (%args) = @_;
    my $prefix = File::Spec->catfile($args{directory}, $args{name});
    return supervise_process(
        binary => $args{binary}, binary_identity => $args{binary_identity},
        stdout_path => "$prefix.stdout", stderr_path => "$prefix.stderr",
        trace_path => $args{trace_path}, command => $args{command},
        environment => $args{environment} // {},
        deadline_us => $args{deadline_us} // 80_000,
        term_grace_us => $args{term_grace_us} // 40_000,
        drain_grace_us => $args{drain_grace_us} // 100_000,
        stdout_limit => $args{stdout_limit} // 16 * 1024,
        stderr_limit => $args{stderr_limit} // 16 * 1024,
        trace_limit => $args{trace_limit} // 16 * 1024,
        rss_limit => $args{rss_limit} // 256 * 1024 * 1024,
        pre_setsid_delay_us => $args{pre_setsid_delay_us},
        pre_setsid_setup => $args{pre_setsid_setup},
        child_setup => $args{child_setup}, rss_sampler => $args{rss_sampler},
    );
}

sub observation_line {
    my ($case, @fields) = @_;
    return join ' ', 'typokat-wu0e-self-test-observation-v1',
        "case=$case", @fields;
}

sub self_test {
    my $root = repo_root();
    my $directory = create_run_directory($root, 'self-tests');
    my $perl = abs_path('/usr/bin/perl') // fail('self-test perl is unavailable');
    my $perl_identity = sha256_hex(read_bytes($perl));
    my @lines;
    my $max_interval = 0;
    my $launch_count = 0;
    my $observe_interval = sub {
        my ($result, $interval_fixture) = @_;
        return unless $interval_fixture;
        $max_interval = $result->{max_rss_sample_interval_us}
            if $result->{max_rss_sample_interval_us} > $max_interval;
    };

    my $setsid = self_test_supervise(
        directory => $directory, name => 'setsid', binary => $perl,
        binary_identity => $perl_identity, command => [$perl, '-e', 'select undef,undef,undef,0.02'],
        deadline_us => 500_000);
    ++$launch_count;
    $observe_interval->($setsid);
    $setsid->{group_ready} && $setsid->{termination} eq 'normal'
        or fail("setsid containment self-test failed; artifacts: $directory");
    push @lines, observation_line('setsid-containment', 'group_isolated=1');

    my $pre = self_test_supervise(
        directory => $directory, name => 'pre-setsid', binary => $perl,
        binary_identity => $perl_identity, command => [$perl, '-e', 'exit 0'],
        deadline_us => 20_000, term_grace_us => 20_000,
        pre_setsid_delay_us => 150_000,
        pre_setsid_setup => sub { $SIG{TERM} = 'IGNORE' });
    ++$launch_count;
    $observe_interval->($pre);
    !$pre->{group_ready} && $pre->{direct_kill_attempted}
        or fail("pre-setsid direct kill self-test failed; artifacts: $directory");
    push @lines, observation_line('pre-setsid-direct-kill', 'direct_kill_attempted=1');

    my $descendant_program = <<'PERL';
$| = 1;
$SIG{TERM} = 'IGNORE';
my $child = fork();
die "fork failed: $!\n" unless defined $child;
if ($child == 0) {
    $SIG{TERM} = 'IGNORE';
    select undef, undef, undef, 10 while 1;
}
print "$child\n";
exit 0;
PERL
    my $descendant = self_test_supervise(
        directory => $directory, name => 'descendant', binary => $perl,
        binary_identity => $perl_identity, command => [$perl, '-e', $descendant_program]);
    ++$launch_count;
    $observe_interval->($descendant);
    $descendant->{zombie_leader_observed}
        or fail("zombie reservation self-test failed; artifacts: $directory");
    push @lines, observation_line('zombie-leader-reservation', 'pgid_reserved=1');
    my ($descendant_stdout) = read_bounded_file(
        File::Spec->catfile($directory, 'descendant.stdout'), 1024);
    my ($descendant_pid) = $descendant_stdout =~ /\A([0-9]+)\n\z/;
    defined $descendant_pid or fail('descendant PID was not captured');
    !live_non_zombie_process($descendant_pid)
        or fail("descendant survived whole-group kill; artifacts: $directory");
    push @lines, observation_line(
        'leader-exit-descendant-kill', 'descendants_reaped=1');

    my $rss_program = <<'PERL';
my $memory = 'x' x (12 * 1024 * 1024);
my $child = fork();
die "fork failed: $!\n" unless defined $child;
if ($child == 0) {
    my $child_memory = 'y' x (12 * 1024 * 1024);
    select undef, undef, undef, 0.08;
    exit(length($child_memory) == 0);
}
select undef, undef, undef, 0.08;
waitpid($child, 0);
exit(length($memory) == 0);
PERL
    my $rss = self_test_supervise(
        directory => $directory, name => 'summed-rss', binary => $perl,
        binary_identity => $perl_identity, command => [$perl, '-e', $rss_program],
        deadline_us => 500_000);
    ++$launch_count;
    $observe_interval->($rss);
    $rss->{termination} eq 'normal'
        && $rss->{max_group_rss_bytes} > $rss->{largest_member_rss_bytes}
        or fail("summed RSS self-test failed; artifacts: $directory");
    push @lines, observation_line('summed-live-group-rss',
        "summed_rss_bytes=$rss->{max_group_rss_bytes}",
        "largest_member_rss_bytes=$rss->{largest_member_rss_bytes}");
    my $interval = self_test_supervise(
        directory => $directory, name => 'rss-interval', binary => $perl,
        binary_identity => $perl_identity,
        command => [$perl, '-e', 'select undef,undef,undef,0.03'],
        deadline_us => 500_000,
        rss_sampler => sub { return (0, 0, undef, 1) });
    ++$launch_count;
    $observe_interval->($interval, 1);
    $interval->{termination} eq 'normal'
        or fail("RSS sample telemetry self-test failed; artifacts: $directory");
    $max_interval <= $MAX_RSS_SAMPLE_INTERVAL_US
        or fail("RSS sample interval self-test failed; artifacts: $directory");
    push @lines, observation_line(
        'rss-sampling-interval', "max_interval_us=$max_interval");

    for my $stream (qw(stdout stderr)) {
        my $program = $stream eq 'stdout'
            ? 'syswrite(STDOUT, "x" x (256 * 1024)) == 256 * 1024 or die $!; $SIG{TERM}="IGNORE"; select undef,undef,undef,10 while 1'
            : 'syswrite(STDERR, "x" x (256 * 1024)) == 256 * 1024 or die $!; $SIG{TERM}="IGNORE"; select undef,undef,undef,10 while 1';
        my $flood = self_test_supervise(
            directory => $directory, name => "$stream-flood", binary => $perl,
            binary_identity => $perl_identity, command => [$perl, '-e', $program],
            deadline_us => 1_000_000,
            stdout_limit => $stream eq 'stdout' ? $MAX_STDOUT_BYTES : 16 * 1024,
            stderr_limit => $stream eq 'stderr' ? $MAX_STDERR_BYTES : 16 * 1024);
        ++$launch_count;
        $observe_interval->($flood);
        my $observed = $stream eq 'stdout'
            ? $flood->{stdout_bytes} : $flood->{stderr_bytes};
        $flood->{termination} eq $stream && $observed > 128 * 1024
            && $flood->{group_kill_attempted}
            or fail("$stream flood self-test failed; artifacts: $directory");
        push @lines, observation_line("$stream-flood",
            "observed_bytes=$observed", 'whole_group_terminated=1');
    }

    my $trace_path = File::Spec->catfile($directory, 'trace-flood.trace');
    my $trace_program =
        'open my $h, ">:raw", $ARGV[0] or die $!; syswrite($h, "x" x (512 * 1024)) == 512 * 1024 or die $!; close $h; $SIG{TERM}="IGNORE"; select undef,undef,undef,10 while 1';
    my $trace = self_test_supervise(
        directory => $directory, name => 'trace-flood', binary => $perl,
        binary_identity => $perl_identity,
        command => [$perl, '-e', $trace_program, $trace_path],
        trace_path => $trace_path, trace_limit => $MAX_TRACE_BYTES,
        deadline_us => 1_000_000);
    ++$launch_count;
    $observe_interval->($trace);
    $trace->{termination} eq 'trace' && $trace->{trace_bytes} > 256 * 1024
        && $trace->{group_kill_attempted}
        or fail("trace flood self-test failed; artifacts: $directory");
    push @lines, observation_line('trace-flood',
        "observed_bytes=$trace->{trace_bytes}", 'whole_group_terminated=1');
    !$trace->{drain_expired}
        or fail("bounded drain self-test failed; artifacts: $directory");
    push @lines, observation_line('bounded-drain', 'drain_expired=0');

    my $oversized_path = File::Spec->catfile($directory, 'post-read.bin');
    write_bytes_exclusive($oversized_path, 'x' x ($MAX_TRACE_BYTES + 2));
    my (undef, $oversized, $read_count) =
        read_bounded_file($oversized_path, $MAX_TRACE_BYTES);
    $oversized && $read_count == $MAX_TRACE_BYTES + 1
        or fail('bounded post-read self-test failed');
    push @lines, observation_line(
        'bounded-post-read', "max_read_bytes=$read_count");

    my $rss_failure = self_test_supervise(
        directory => $directory, name => 'rss-failure', binary => $perl,
        binary_identity => $perl_identity,
        command => [$perl, '-e', '$SIG{TERM}="IGNORE"; select undef,undef,undef,10 while 1'],
        rss_sampler => sub { return (undef, undef, 'synthetic RSS read failure') });
    ++$launch_count;
    $observe_interval->($rss_failure);
    $rss_failure->{termination} eq 'infrastructure'
        or fail("RSS sampling failure self-test failed; artifacts: $directory");
    push @lines, observation_line('rss-sampling-failure',
        'termination=infrastructure', 'rss_assumed_zero=0');
    !defined checked_mul($MAX_SAFE_INTEGER, 2)
        && !defined checked_add($MAX_SAFE_INTEGER, 1)
        or fail('RSS overflow self-test failed');
    my $rss_overflow = self_test_supervise(
        directory => $directory, name => 'rss-overflow', binary => $perl,
        binary_identity => $perl_identity,
        command => [$perl, '-e', '$SIG{TERM}="IGNORE"; select undef,undef,undef,10 while 1'],
        rss_sampler => sub { return (undef, undef, 'RSS group sum overflow') });
    ++$launch_count;
    $observe_interval->($rss_overflow);
    $rss_overflow->{termination} eq 'infrastructure'
        or fail("RSS arithmetic overflow containment failed; artifacts: $directory");
    push @lines, observation_line('rss-arithmetic-overflow',
        'overflow_detected=1', 'termination=infrastructure');

    my $swap = File::Spec->catfile($directory, 'binary-swap.pl');
    write_bytes_exclusive($swap, <<'PERL');
#!/usr/bin/env perl
chmod 0700, $0 or die $!;
open my $h, '>>:raw', $0 or die $!;
print {$h} "#changed\n" or die $!;
close $h or die $!;
PERL
    chmod 0500, $swap or fail("cannot chmod swap binary: $!");
    my $swap_identity = sha256_hex(read_bytes($swap));
    my $swap_error = '';
    eval {
        self_test_supervise(
            directory => $directory, name => 'binary-swap', binary => $swap,
            binary_identity => $swap_identity, command => [$swap], deadline_us => 500_000);
        1;
    } or $swap_error = $@;
    $swap_error =~ /frozen libtest digest mismatch/
        or fail("binary swap self-test failed; artifacts: $directory");
    ++$launch_count;
    push @lines, observation_line('binary-swap', 'replacement_rejected=1');

    my $env_program =
        'print join("\n", sort grep { /\\ATYPOKAT_WU0[B-E]_/ } keys %ENV), "\n"';
    local $ENV{TYPOKAT_WU0B_OLD} = '1';
    local $ENV{TYPOKAT_WU0C_OLD} = '1';
    local $ENV{TYPOKAT_WU0D_OLD} = '1';
    local $ENV{TYPOKAT_WU0E_OLD} = '1';
    my $env = self_test_supervise(
        directory => $directory, name => 'environment', binary => $perl,
        binary_identity => $perl_identity, command => [$perl, '-e', $env_program],
        deadline_us => 500_000,
        environment => {
            TYPOKAT_WU0E_MODE => 'plain',
            TYPOKAT_WU0E_TRACE_PATH => '/tmp/wu0e-self-test.trace',
        });
    ++$launch_count;
    $observe_interval->($env);
    my ($env_stdout) = read_bounded_file(
        File::Spec->catfile($directory, 'environment.stdout'), 1024);
    $env_stdout eq "TYPOKAT_WU0E_MODE\nTYPOKAT_WU0E_TRACE_PATH\n"
        or fail("environment scrub self-test failed; artifacts: $directory");
    push @lines, observation_line('environment-scrub', 'removed_variable_count=4');
    push @lines, observation_line('workload-allowlist', 'exact_variable_count=2');

    my $validator_env = self_test_supervise(
        directory => $directory, name => 'validator-environment', binary => $perl,
        binary_identity => $perl_identity, command => [$perl, '-e', $env_program],
        deadline_us => 500_000,
        environment => {
            TYPOKAT_WU0E_VALIDATE_TRACE_PATH => '/tmp/wu0e-self-test.trace',
            TYPOKAT_WU0E_VALIDATE_MODE => 'plain',
            TYPOKAT_WU0E_VALIDATE_TERMINATION => 'normal',
        });
    ++$launch_count;
    $observe_interval->($validator_env);
    my ($validator_env_stdout) = read_bounded_file(
        File::Spec->catfile($directory, 'validator-environment.stdout'), 1024);
    my @validator_keys = grep { $_ ne '' } split /\n/, $validator_env_stdout;
    @validator_keys == 3
        or fail("validator allowlist self-test failed; artifacts: $directory");
    push @lines, observation_line('validator-allowlist', 'exact_variable_count=3');

    my @schedule = map { ("workload:$_", "validator:$_") } @MODES;
    join(',', @schedule) eq
        'workload:plain,validator:plain,workload:measured-off,validator:measured-off,workload:candidate-b,validator:candidate-b'
        or fail('validator adjacency schedule changed');
    push @lines, observation_line('validator-after-each-workload',
        'workload_count=3', 'validator_count=3',
        'validator_immediately_after_workload=1');
    my @workload_command = workload_command('/frozen/libtest');
    $workload_command[0] eq '/frozen/libtest'
        && $workload_command[3] eq $WORKLOAD_PROBE
        or fail('primary probe command changed');
    push @lines, observation_line('exact-primary-probe',
        'compiler_command=frozen-libtest');
    push @lines, observation_line('no-alternate-compiler',
        'alternate_exec_observed=0');
    push @lines, observation_line('same-binary-validator',
        'binary_identity_count=1');
    for (1 .. 6) {
        assert_expected_binary($perl, $perl_identity);
        assert_expected_binary($perl, $perl_identity);
    }
    push @lines, observation_line('pre-post-binary-digest', 'verified_launches=6');
    push @lines, observation_line('one-frozen-binary', 'build_count=1');

    my $inventory = strict_profile_inventory($root);
    my $inventory_identity = $inventory->{inventory_identity};
    for (2 .. 6) {
        my $again = strict_profile_inventory($root);
        $again->{regular_files} == 88
            && $again->{inventory_identity} eq $inventory_identity
            or fail('warm inventory identity changed across simulated launches');
    }
    $inventory->{regular_files} == 88 or fail('warm inventory self-test failed');
    push @lines, observation_line('warm-inventory-before-every-launch', 'warm_count=6');
    push @lines, observation_line('same-binary-host-profile-inventory',
        'identity_tuple_count=1');
    push @lines, observation_line('cross-mode-identity-parity', 'parity=1');

    $launch_count >= 10 or fail('behavioral self-test launch inventory is incomplete');
    $max_interval <= $MAX_RSS_SAMPLE_INTERVAL_US
        or fail("observed RSS interval exceeded contract; artifacts: $directory");
    print join("\n", @lines), "\n";
    print join(' ', 'typokat-wu0e-self-test-v1', 'result=ok',
        "deadline_us=$DEADLINE_US",
        "max_process_group_rss_bytes=$MAX_PROCESS_GROUP_RSS_BYTES",
        "max_stdout_bytes=$MAX_STDOUT_BYTES",
        "max_stderr_bytes=$MAX_STDERR_BYTES",
        "max_trace_bytes=$MAX_TRACE_BYTES",
        "max_observed_rss_sample_interval_us=$max_interval"), "\n";
    remove_tree($directory);
}

sub main {
    my @arguments = @ARGV;
    if (@arguments == 1 && $arguments[0] eq '--internal-marker-probe') {
        ensure_delegated_scope(@arguments);
        fail('internal marker probe unexpectedly passed');
    }
    if (@arguments == 4 && $arguments[0] eq '--self-test-evidence') {
        my $scope = ensure_delegated_scope(@arguments);
        hardening_self_test_evidence(
            scope => $scope, evidence => $arguments[1], fixtures => $arguments[2],
            nonce => $arguments[3]);
        return;
    }
    if (@arguments == 1 && $arguments[0] eq '--self-test') {
        self_test();
        return;
    }
    if (@arguments == 1 && ($arguments[0] eq '--help' || $arguments[0] eq '-h')) {
        print usage();
        return;
    }
    my $dry_run = @arguments == 1 && $arguments[0] eq '--dry-run';
    @arguments == 0 || $dry_run or fail("invalid arguments\n" . usage());
    my $delegated_identity = $dry_run
        ? undef : ensure_delegated_scope(@arguments);

    my $root = repo_root();
    chdir $root or fail("cannot chdir to $root: $!");
    my $inventory = strict_profile_inventory($root);
    if ($dry_run) {
        print join(' ', 'typokat-wu0e-runner-dry-v1',
            'mode_order=plain,measured-off,candidate-b',
            'build_count=1', 'workload_count=3', 'validator_count=3',
            "profile_files=$inventory->{source_count}",
            "warm_regular_files=$inventory->{regular_files}",
            "deadline_us=$DEADLINE_US",
            "max_process_group_rss_bytes=$MAX_PROCESS_GROUP_RSS_BYTES",
            "max_stdout_bytes=$MAX_STDOUT_BYTES",
            "max_stderr_bytes=$MAX_STDERR_BYTES",
            "max_trace_bytes=$MAX_TRACE_BYTES"), "\n";
        return;
    }

    my ($scope, $run_dir, $success_line, $teardown_done, $scope_abort_requested);
    my $run_ok = eval {
        $scope = setup_delegated_root($delegated_identity);
        my $coordinator = hardened_linux_process_stat($$)
            // fail('cannot inspect production coordinator identity');
        $run_dir = create_run_directory($root, 'runs');
        my $built = build_release_libtest_once($root, $run_dir);
        my ($binary, $binary_identity) = freeze_libtest($root, $built);
        my ($host_facts, $host_identity) = host_facts();
        write_bytes_exclusive(
            File::Spec->catfile($run_dir, 'host-facts.txt'), $host_facts);
        my $facts = join "\n",
            'typokat-wu0e-run-facts-v2',
            "binary=$binary", "binary_identity=$binary_identity",
            "host_identity=$host_identity", "profile_identity=$PROFILE_IDENTITY",
            "inventory_identity=$inventory->{inventory_identity}",
            "scope_unit=$scope->{unit}",
            "scope_control_group=$scope->{control_group}",
            'mode_order=plain,measured-off,candidate-b', '';
        write_bytes_exclusive(
            File::Spec->catfile($run_dir, 'run-facts.txt'), $facts);

        my $schedule = run_shared_mode_scheduler(
            workload => sub {
                my ($mode) = @_;
                return run_hardened_workload(
                    scope => $scope, root => $root, run_dir => $run_dir,
                    binary => $binary, binary_identity => $binary_identity,
                    host_identity => $host_identity,
                    inventory_identity => $inventory->{inventory_identity},
                    mode => $mode,
                    scope_abort_requested_callback => sub {
                        $scope_abort_requested = 1;
                    });
            },
            validator => sub {
                my ($mode, $process) = @_;
                my ($validator_launched, $validation) = (0, undef);
                my $validator_ok = eval {
                    $validation = run_hardened_validator(
                        scope => $scope, root => $root, run_dir => $run_dir,
                        binary => $binary, binary_identity => $binary_identity,
                        host_identity => $host_identity,
                        inventory_identity => $inventory->{inventory_identity},
                        mode => $mode, trace_path => $process->{_trace_path},
                        termination => $process->{termination},
                        scope_abort_requested_callback => sub {
                            $scope_abort_requested = 1;
                        },
                        launch_confirmed_callback => sub {
                            $validator_launched = 1;
                            write_hardened_result_meta($process, 1);
                        });
                    1;
                };
                if (!$validator_ok) {
                    my $validator_error = $@;
                    write_hardened_result_meta($process, 0)
                        unless $validator_launched;
                    die $validator_error;
                }
                return $validation;
            },
            stop => sub {
                my ($mode, $process) = @_;
                write_hardened_result_meta($process, 0);
                fail("$mode workload failed with $process->{termination}; artifacts: $run_dir");
            });
        $schedule->{stopped} and fail('shared scheduler stopped without failure');
        my @dossier_observations = map {
            production_dossier_observation(
                $_->{mode}, $_->{process}, $_->{validation})
        } @{ $schedule->{observations} };
        my $dossier_bytes = dossier_v2_bytes(
            binary_identity => $binary_identity, host_identity => $host_identity,
            profile_identity => $PROFILE_IDENTITY,
            inventory_identity => $inventory->{inventory_identity},
            observations => \@dossier_observations);
        my $dossier = File::Spec->catfile(
            $run_dir, 'diagnostic-dossier-v2.txt');
        write_bytes_exclusive($dossier, $dossier_bytes);
        write_bytes_exclusive(
            File::Spec->catfile($run_dir, 'diagnostic-dossier-v2.sha256'),
            sha256_hex($dossier_bytes) . "\n");
        my $controllers_after = teardown_delegated_root($scope);
        $teardown_done = 1;
        write_delegation_evidence(
            evidence => $run_dir, scope => $scope, route => 'production',
            controllers_after => $controllers_after, coordinator_pid => $$,
            coordinator_start_ticks => $coordinator->{start_ticks});
        $success_line =
            "typokat-wu0e-runner-v2 result=ok modes=3 binary_identity=$binary_identity "
            . "host_identity=$host_identity dossier=$dossier artifacts=$run_dir\n";
        1;
    };
    if (!$run_ok) {
        my $primary_error = $@;
        if (!$scope_abort_requested
            && defined $scope && !$teardown_done && -d $scope->{supervisor}) {
            my $teardown_ok = eval {
                teardown_delegated_root($scope);
                1;
            };
            $primary_error .= $@ unless $teardown_ok;
        }
        die $primary_error;
    }
    print $success_line;
}

main();
