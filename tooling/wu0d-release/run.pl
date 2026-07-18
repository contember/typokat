#!/usr/bin/env perl

use v5.20;
use strict;
use warnings;

use Cwd qw(abs_path);
use Digest::SHA qw(sha256_hex);
use Encode qw(encode);
use Errno qw(EAGAIN EWOULDBLOCK);
use File::Basename qw(dirname);
use File::Copy qw(copy);
use File::Path qw(make_path remove_tree);
use File::Spec;
use Fcntl qw(:DEFAULT F_GETFL F_SETFL FD_CLOEXEC F_GETFD F_SETFD);
use IO::Handle ();
use JSON::PP qw(decode_json);
use POSIX qw(WNOHANG setsid strftime);
use Time::HiRes qw(CLOCK_MONOTONIC clock_gettime usleep);

my $TIMEOUT_SECONDS = 5;
my $TERM_GRACE_US = 250_000;
my $DRAIN_GRACE_US = 250_000;
my $MAX_STDOUT_BYTES = 128 * 1024;
my $MAX_STDERR_BYTES = 128 * 1024;
my $MAX_TIME_BYTES = 4 * 1024;
my $PROFILE_IDENTITY = 'ea59b3e150195f6cfe843661c0bcb006cffb04dd988861778a188be9441c579d';
my $PROFILE_MANIFEST_SHA256 = '1edef1b5e870024834762267ec532c3054f3b2279e9181844e21648243eb1407';
my $CANDIDATE_VALUE = 'candidate-b-v1';
my $PROFILE_GITATTRIBUTES = "lib/*.d.ts -text -diff\n"
    . "LICENSE.txt -text -diff\n"
    . "ThirdPartyNoticeText.txt -text -diff\n"
    . "profile.toml text eol=lf\n"
    . "README.md text eol=lf\n"
    . "THIRD_PARTY_NOTICE.md text eol=lf\n";

my %WORKLOAD = (
    primary => {
        profile => $PROFILE_IDENTITY,
        probe => 'check::checker::wu0d_candidate_release::wu0d_candidate_primary_probe_once',
    },
    'non-cycle' => {
        profile => '1c664166f4c307f032958836642008c90c28cb21ff33215144c9188ac8afdd19',
        probe => 'check::checker::wu0d_candidate_release::wu0d_candidate_non_cycle_probe_once',
    },
    'reporter-control' => {
        profile => '9f5e4ab6a334154e67fe1ead6e7e1038d9433f5fd66f7ed897a73cfd1d058d0b',
        probe => 'check::checker::wu0d_candidate_release::wu0d_candidate_reporter_control_probe_once',
    },
);

my %CONTROL_FIXTURE = (
    'non-cycle' => 'NON_CYCLE_WORKLOAD_SOURCES',
    'reporter-control' => 'REPORTER_CONTROL_WORKLOAD_SOURCES',
);

sub usage {
    return <<'USAGE';
Usage:
  perl tooling/wu0d-release/run.pl --smoke-control
  perl tooling/wu0d-release/run.pl --single primary off
  perl tooling/wu0d-release/run.pl --single WORKLOAD MODE
  perl tooling/wu0d-release/run.pl --full
  perl tooling/wu0d-release/run.pl --dry-run [--single WORKLOAD MODE]
  perl tooling/wu0d-release/run.pl --self-test

WORKLOAD is primary, non-cycle, or reporter-control.
MODE is off or candidate-b. --smoke-control means --single non-cycle off.
USAGE
}

sub fail {
    die "wu0d-release: $_[0]\n";
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

sub file_size {
    my ($path) = @_;
    my @stat = lstat $path;
    return 0 unless @stat;
    -f _ && !-l _ or fail("captured output is not a regular non-symlink file: $path");
    return $stat[7];
}

sub read_bounded_file {
    my ($path, $limit) = @_;
    return ('', 0) unless -e $path;
    -f $path && !-l $path
        or fail("captured output is not a regular non-symlink file: $path");
    open my $handle, '<:raw', $path or fail("cannot open $path: $!");
    my $bytes = '';
    while (length($bytes) <= $limit) {
        my $remaining = $limit + 1 - length($bytes);
        my $chunk = '';
        my $count = sysread($handle, $chunk, $remaining);
        defined $count or fail("cannot read $path: $!");
        last if $count == 0;
        $bytes .= $chunk;
    }
    close $handle or fail("cannot close $path: $!");
    return ('', 1) if length($bytes) > $limit;
    return ($bytes, 0);
}

sub linux_process_stat {
    my ($pid) = @_;
    my $path = "/proc/$pid/stat";
    open my $handle, '<:raw', $path or return;
    my $bytes = '';
    my $count = sysread($handle, $bytes, 4097);
    close $handle;
    return unless defined $count && $count > 0 && $count <= 4096;
    my ($actual_pid, $state, $pgrp) = $bytes =~ /\A([0-9]+) \(.*\) ([A-Z]) [0-9]+ ([0-9]+) /;
    return unless defined $actual_pid && $actual_pid == $pid;
    return { state => $state, pgrp => 0 + $pgrp };
}

sub live_process_group_members {
    my ($pgrp) = @_;
    opendir my $proc, '/proc' or fail("cannot inspect /proc: $!");
    my @pids = grep { /\A[0-9]+\z/ } readdir $proc;
    closedir $proc or fail("cannot close /proc: $!");
    my @live;
    for my $pid (@pids) {
        my $stat = linux_process_stat($pid) // next;
        push @live, 0 + $pid
            if $stat->{pgrp} == $pgrp && $stat->{state} ne 'Z' && $stat->{state} ne 'X';
    }
    return @live;
}

sub assert_expected_binary {
    my ($path, $expected_sha) = @_;
    defined $expected_sha && $expected_sha =~ /\A[0-9a-f]{64}\z/
        or fail('expected binary identity is not canonical');
    -f $path && -x $path && !-l $path
        or fail("frozen libtest is not an executable regular non-symlink file: $path");
    sha256_hex(read_bytes($path)) eq $expected_sha
        or fail("frozen libtest digest mismatch: $path");
}

sub write_bytes_exclusive {
    my ($path, $bytes) = @_;
    sysopen my $handle, $path, O_WRONLY | O_CREAT | O_EXCL, 0600
        or fail("cannot create $path: $!");
    binmode $handle, ':raw';
    print {$handle} $bytes or fail("cannot write $path: $!");
    close $handle or fail("cannot close $path: $!");
}

sub shell_quote {
    my ($value) = @_;
    return "'" . ($value =~ s/'/'"'"'/gr) . "'";
}

sub lower_hex {
    return unpack 'H*', $_[0];
}

sub command_text {
    return join ' ', map { shell_quote($_) } @_;
}

sub run_captured {
    my ($stdout_path, $stderr_path, @command) = @_;
    my $pid = fork();
    defined $pid or fail("fork failed for @command: $!");
    if ($pid == 0) {
        open STDOUT, '>:raw', $stdout_path or die "cannot redirect stdout: $!\n";
        open STDERR, '>:raw', $stderr_path or die "cannot redirect stderr: $!\n";
        exec { $command[0] } @command;
        die "cannot exec $command[0]: $!\n";
    }
    waitpid($pid, 0) == $pid or fail("waitpid failed for @command: $!");
    return decode_wait_status($?);
}

sub decode_wait_status {
    my ($status) = @_;
    return 128 + ($status & 127) if $status & 127;
    return ($status >> 8) & 255;
}

sub repo_root {
    my $script = abs_path($0) // fail("cannot resolve script path $0");
    my $root = abs_path(File::Spec->catdir(dirname($script), '..', '..'))
        // fail('cannot resolve repository root');
    -f File::Spec->catfile($root, 'Cargo.toml')
        or fail("repository root has no Cargo.toml: $root");
    return $root;
}

sub canonical_length_framed_sha256 {
    my ($records) = @_;
    ref($records) eq 'ARRAY' or fail('length-framed records must be an array');
    my $digest = Digest::SHA->new(256);
    for my $record (@$records) {
        ref($record) eq 'ARRAY' && @$record == 2
            or fail('length-framed record must contain name and source');
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

sub require_real_directory {
    my ($path) = @_;
    my @stat = lstat $path;
    @stat && -d _ && !-l _
        or fail("profile path is not a real directory: $path");
}

sub read_regular_input {
    my ($path) = @_;
    my @stat = lstat $path;
    @stat && -f _ && !-l _
        or fail("profile input is not a regular non-symlink file: $path");
    return read_bytes($path);
}

sub assert_exact_inventory_names {
    my ($actual, $expected, $label) = @_;
    my @actual = sort @$actual;
    my @expected = sort @$expected;
    join("\0", @actual) eq join("\0", @expected)
        or fail("profile directory inventory changed: $label");
}

sub collect_profile_runtime_tree {
    my ($profile_root) = @_;
    require_real_directory($profile_root);
    opendir my $root_dir, $profile_root
        or fail("cannot traverse profile directory $profile_root: $!");
    my (@root_files, @root_dirs);
    for my $name (grep { $_ ne '.' && $_ ne '..' } readdir $root_dir) {
        my $path = File::Spec->catfile($profile_root, $name);
        my @stat = lstat $path;
        @stat && !-l _ or fail("profile tree contains a symlink or vanished entry: $path");
        if (-d _) {
            push @root_dirs, $name;
        } elsif (-f _) {
            push @root_files, $name;
        } else {
            fail("profile tree contains a non-regular entry: $path");
        }
    }
    closedir $root_dir or fail("cannot close profile directory $profile_root: $!");
    assert_exact_inventory_names(\@root_dirs, ['lib'], "$profile_root directories");

    my $library_root = File::Spec->catdir($profile_root, 'lib');
    require_real_directory($library_root);
    opendir my $library_dir, $library_root
        or fail("cannot traverse profile directory $library_root: $!");
    my @library_files;
    for my $name (grep { $_ ne '.' && $_ ne '..' } readdir $library_dir) {
        my $path = File::Spec->catfile($library_root, $name);
        my @stat = lstat $path;
        @stat && -f _ && !-l _
            or fail("profile library entry is not a regular non-symlink file: $path");
        push @library_files, $name;
    }
    closedir $library_dir or fail("cannot close profile directory $library_root: $!");
    return (\@root_files, \@library_files);
}

sub strict_profile_inventory {
    my ($root) = @_;
    my $profile_root = File::Spec->catdir($root, 'src', 'library', 'typescript-6.0.3');
    my $manifest_path = File::Spec->catfile($profile_root, 'profile.toml');
    my ($actual_root_files, $actual_library_files) =
        collect_profile_runtime_tree($profile_root);
    my $manifest = read_regular_input($manifest_path);
    sha256_hex($manifest) eq $PROFILE_MANIFEST_SHA256
        or fail('profile.toml fingerprint changed');
    $manifest !~ /\r/ or fail('profile.toml contains CR bytes');
    $manifest =~ /\n\z/ or fail('profile.toml has no final LF');
    $manifest =~ /^file_count = 82$/m or fail('profile.toml does not pin file_count = 82');
    $manifest =~ /^source_bytes = 2936611$/m or fail('profile.toml source byte count changed');
    $manifest =~ /^length_framed_sha256 = "\Q$PROFILE_IDENTITY\E"$/m
        or fail('profile.toml registry identity changed');

    my @sections = split /^\[\[file\]\]\n/m, $manifest;
    shift @sections;
    @sections == 82 or fail('profile.toml must contain exactly 82 file records');

    my @source_specs;
    my @source_names;
    for my $ordinal (0 .. $#sections) {
        my $section = $sections[$ordinal];
        my ($actual_ordinal) = $section =~ /^ordinal = ([0-9]+)$/m;
        my ($name) = $section =~ /^name = "([a-z0-9.]+\.d\.ts)"$/m;
        my ($expected_bytes) = $section =~ /^bytes = ([0-9]+)$/m;
        my ($expected_sha) = $section =~ /^sha256 = "([0-9a-f]{64})"$/m;
        defined $actual_ordinal && $actual_ordinal == $ordinal
            or fail("profile file ordinal $ordinal is missing or reordered");
        defined $name && defined $expected_bytes && defined $expected_sha
            or fail("profile file record $ordinal is incomplete");
        push @source_names, $name;
        my $path = File::Spec->catfile($profile_root, 'lib', $name);
        push @source_specs, [$name, $path, 0 + $expected_bytes, $expected_sha];
    }

    my @root_files = qw(
        .gitattributes
        LICENSE.txt
        README.md
        THIRD_PARTY_NOTICE.md
        ThirdPartyNoticeText.txt
        profile.toml
    );
    assert_exact_inventory_names($actual_root_files, \@root_files, "$profile_root files");
    assert_exact_inventory_names(
        $actual_library_files, \@source_names, "$profile_root/lib files");

    my %root_bytes;
    for my $name (@root_files) {
        my $path = File::Spec->catfile($profile_root, $name);
        $root_bytes{$name} = $name eq 'profile.toml' ? $manifest : read_regular_input($path);
    }
    $root_bytes{'.gitattributes'} eq $PROFILE_GITATTRIBUTES
        or fail('the profile .gitattributes contract changed');
    length($root_bytes{'LICENSE.txt'}) == 9_197
        && sha256_hex($root_bytes{'LICENSE.txt'}) eq
            'a7d00bfd54525bc694b6e32f64c7ebcf5e6b7ae3657be5cc12767bce74654a47'
        or fail('profile license bytes changed');
    length($root_bytes{'ThirdPartyNoticeText.txt'}) == 37_824
        && sha256_hex($root_bytes{'ThirdPartyNoticeText.txt'}) eq
            '1af3c68039c57e539422da82a4faada506ce6d0ea6f90e0b699d02dbcdb7a90c'
        or fail('profile third-party notice bytes changed');

    my (@sources, @records);
    my $total_bytes = 0;
    for my $spec (@source_specs) {
        my ($name, $path, $expected_bytes, $expected_sha) = @$spec;
        my $bytes = read_regular_input($path);
        length($bytes) == $expected_bytes
            or fail("profile source byte count changed: $name");
        sha256_hex($bytes) eq $expected_sha
            or fail("profile source digest changed: $name");
        push @records, [$name, $bytes];
        push @sources, $path;
        $total_bytes += length($bytes);
    }
    $total_bytes == 2_936_611 or fail('profile source total changed');
    canonical_length_framed_sha256(\@records) eq $PROFILE_IDENTITY
        or fail('profile registry digest changed');

    my @all_files = (
        map { File::Spec->catfile($profile_root, $_) } @root_files,
        @sources,
    );
    @all_files == 88 or fail('strict profile runtime inventory must contain 88 files');
    my $warmed_bytes = 0;
    $warmed_bytes += length($_) for values %root_bytes;
    $warmed_bytes += $total_bytes;
    return {
        profile_root => $profile_root,
        source_count => scalar(@sources),
        all_files => \@all_files,
        warmed_bytes => $warmed_bytes,
    };
}

sub extract_control_fixture {
    my ($rust, $constant) = @_;
    my $literal = qr/"(?:\\["\\\/bfnrt]|\\u[0-9a-fA-F]{4}|[^"\\\x00-\x1f])*"/;
    my $marker = "pub(super) const $constant:";
    my $start = index($rust, $marker);
    $start >= 0 && index($rust, $marker, $start + 1) < 0
        or fail("Rust control fixture declaration is missing or duplicated: $constant");
    my $tail = substr($rust, $start);
    my ($declaration) = $tail =~ /\A(.*?^\s*\}\];)/ms;
    defined $declaration or fail("Rust control fixture declaration is unterminated: $constant");
    my ($name_literal, $source_literal) = $declaration =~
        /\A\Q$marker\E[^=]*=\s*&\[\s*InjectedLibrarySource\s*\{\s*file_ordinal:\s*LibraryFileOrdinal::new\(0\),\s*name:\s*($literal),\s*source:\s*($literal),?\s*\}\s*\];\z/s;
    defined $name_literal && defined $source_literal
        or fail("Rust control fixture shape changed: $constant");
    my $name = eval { decode_json($name_literal) };
    defined $name && !$@ or fail("Rust control fixture name is not a supported string: $constant");
    my $source = eval { decode_json($source_literal) };
    defined $source && !$@ or fail("Rust control fixture source is not a supported string: $constant");
    return [encode('UTF-8', $name), encode('UTF-8', $source)];
}

sub verify_control_fixture_bytes {
    my ($rust) = @_;
    for my $workload ('non-cycle', 'reporter-control') {
        my $record = extract_control_fixture($rust, $CONTROL_FIXTURE{$workload});
        canonical_length_framed_sha256([$record]) eq $WORKLOAD{$workload}{profile}
            or fail("$workload Rust fixture identity changed");
    }
}

sub verify_control_fixtures {
    my ($root) = @_;
    my $path = File::Spec->catfile(
        $root, 'src', 'check', 'checker', 'wu0d_candidate_release.rs');
    verify_control_fixture_bytes(read_regular_input($path));
}

sub validate_and_warm_runtime_inputs {
    my ($root, $binary, $binary_identity) = @_;
    assert_expected_binary($binary, $binary_identity);
    verify_control_fixtures($root);
    my $inventory = strict_profile_inventory($root);
    $inventory->{warmed_bytes} > 2_936_611
        or fail('strict profile warmup byte count is incomplete');
    assert_expected_binary($binary, $binary_identity);
    return {
        regular_files => scalar(@{ $inventory->{all_files} }),
        bytes => $inventory->{warmed_bytes},
    };
}

sub create_run_directory {
    my ($root) = @_;
    my $base = File::Spec->catdir($root, 'target', 'wu0d-release', 'runs');
    make_path($base, { mode => 0700 });
    my $stamp = strftime('%Y%m%dT%H%M%SZ', gmtime());
    my $path = File::Spec->catdir($base, "$stamp-$$");
    mkdir $path, 0700 or fail("cannot create run directory $path: $!");
    return $path;
}

sub build_release_libtest_once {
    my ($root, $run_dir) = @_;
    my @command = ('cargo', 'test', '--release', '--lib', '--no-run',
        '--message-format=json-render-diagnostics');
    write_bytes_exclusive(File::Spec->catfile($run_dir, 'build-command.txt'),
        command_text(@command) . "\n");
    my $stdout = File::Spec->catfile($run_dir, 'cargo-build.jsonl');
    my $stderr = File::Spec->catfile($run_dir, 'cargo-build.stderr');
    my $exit = run_captured($stdout, $stderr, @command);
    $exit == 0 or fail("release libtest build failed with exit $exit; artifacts: $run_dir");

    my @executables;
    for my $line (split /\n/, read_bytes($stdout)) {
        next if $line eq '';
        my $message = eval { decode_json($line) };
        defined $message or fail("cargo emitted non-JSON stdout; artifacts: $run_dir");
        next unless ($message->{reason} // '') eq 'compiler-artifact';
        next unless ref($message->{target}) eq 'HASH';
        next unless ($message->{target}{name} // '') eq 'typokat';
        next unless grep { $_ eq 'lib' } @{ $message->{target}{kind} // [] };
        next unless ref($message->{profile}) eq 'HASH' && $message->{profile}{test};
        next unless defined $message->{executable};
        push @executables, $message->{executable};
    }
    @executables == 1
        or fail('cargo JSON did not identify exactly one release libtest executable');
    my $built = abs_path($executables[0])
        // fail("cargo libtest executable does not exist: $executables[0]");
    -f $built && -x $built && !-l $built
        or fail("cargo libtest is not an executable regular non-symlink file: $built");
    return $built;
}

sub freeze_libtest {
    my ($root, $built) = @_;
    my $bytes = read_bytes($built);
    my $digest = sha256_hex($bytes);
    my $freeze_dir = File::Spec->catdir($root, 'target', 'wu0d-release', 'frozen');
    make_path($freeze_dir, { mode => 0700 });
    my $frozen = File::Spec->catfile($freeze_dir, "typokat-libtest-$digest");
    if (-e $frozen) {
        -f $frozen && !-l $frozen or fail("frozen libtest path is unsafe: $frozen");
        sha256_hex(read_bytes($frozen)) eq $digest
            or fail("existing frozen libtest digest mismatch: $frozen");
        chmod 0500, $frozen or fail("cannot lock frozen libtest permissions $frozen: $!");
    } else {
        my $temporary = "$frozen.tmp-$$";
        copy($built, $temporary) or fail("cannot freeze libtest to $temporary: $!");
        chmod 0500, $temporary or fail("cannot chmod frozen libtest $temporary: $!");
        sha256_hex(read_bytes($temporary)) eq $digest
            or fail('frozen libtest changed during copy');
        rename $temporary, $frozen or fail("cannot publish frozen libtest $frozen: $!");
    }
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
    @uname == 5 or fail('POSIX::uname returned an unexpected result');
    my $machine_id = read_bytes('/etc/machine-id');
    my $boot_id = read_bytes('/proc/sys/kernel/random/boot_id');
    $machine_id =~ s/\n\z//;
    $boot_id =~ s/\n\z//;
    $machine_id =~ /\A[0-9a-f]{32}\z/ or fail('/etc/machine-id is not canonical');
    $boot_id =~ /\A[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\z/
        or fail('/proc boot_id is not canonical');
    my $cpu = -f '/proc/cpuinfo' ? read_bytes('/proc/cpuinfo') : '';
    my @models = $cpu =~ /^(?:model name|Hardware)\s*:\s*(.+)$/mg;
    my %seen;
    @models = grep { !$seen{$_}++ } @models;
    my $facts = join "\n",
        'typokat-wu0d-host-v1',
        "sysname=$uname[0]",
        "nodename=$uname[1]",
        "release=$uname[2]",
        "version=$uname[3]",
        "machine=$uname[4]",
        "machine_id=$machine_id",
        "boot_id=$boot_id",
        'cpu_models=' . join('|', @models),
        'logical_cpus=' . scalar(grep { /^processor\s*:/ } split /\n/, $cpu),
        'rustc=' . capture_command_line('rustc', '-Vv'),
        'cargo=' . capture_command_line('cargo', '-V'),
        'time=' . capture_command_line('/usr/bin/time', '--version'),
        '';
    return ($facts, sha256_hex($facts));
}

sub decimal_seconds_to_us {
    my ($seconds) = @_;
    $seconds =~ /\A([0-9]+)(?:\.([0-9]+))?\z/ or fail("invalid elapsed seconds: $seconds");
    my ($whole, $fraction) = ($1, $2 // '');
    length($fraction) <= 9 or fail("elapsed precision is unsupported: $seconds");
    my $nanoseconds = $whole * 1_000_000_000;
    $nanoseconds += $fraction * (10 ** (9 - length($fraction))) if length($fraction);
    return int(($nanoseconds + 999) / 1_000);
}

sub supervise_process {
    my (%args) = @_;
    my $timeout_seconds = $args{timeout_seconds} // $TIMEOUT_SECONDS;
    my $term_grace_us = $args{term_grace_us} // $TERM_GRACE_US;
    my $drain_grace_us = $args{drain_grace_us} // $DRAIN_GRACE_US;
    my $stdout_limit = $args{stdout_limit} // $MAX_STDOUT_BYTES;
    my $stderr_limit = $args{stderr_limit} // $MAX_STDERR_BYTES;
    my @command = @{ $args{command} };
    @command or fail('supervisor received an empty command');

    assert_expected_binary($args{binary}, $args{binary_identity});
    pipe(my $ready_reader, my $ready_writer) or fail("readiness pipe failed: $!");
    fcntl($ready_writer, F_SETFD, fcntl($ready_writer, F_GETFD, 0) | FD_CLOEXEC)
        or fail("cannot set readiness pipe close-on-exec: $!");
    fcntl($ready_reader, F_SETFL, fcntl($ready_reader, F_GETFL, 0) | O_NONBLOCK)
        or fail("cannot make readiness pipe nonblocking: $!");

    my $started = clock_gettime(CLOCK_MONOTONIC);
    my $pid = fork();
    defined $pid or fail("fork failed for supervised process: $!");
    if ($pid == 0) {
        close $ready_reader;
        open STDOUT, '>:raw', $args{stdout_path} or die "cannot redirect stdout: $!\n";
        open STDERR, '>:raw', $args{stderr_path} or die "cannot redirect stderr: $!\n";
        $args{pre_setsid_setup}->() if $args{pre_setsid_setup};
        if ($args{pre_setsid_delay_us}) {
            my $delay_until = clock_gettime(CLOCK_MONOTONIC)
                + $args{pre_setsid_delay_us} / 1_000_000;
            while (1) {
                my $remaining = $delay_until - clock_gettime(CLOCK_MONOTONIC);
                last if $remaining <= 0;
                usleep(int($remaining * 1_000_000));
            }
        }
        setsid() >= 0 or die "setsid failed: $!\n";
        syswrite($ready_writer, 'R') == 1 or die "cannot publish process-group readiness: $!\n";
        close $ready_writer or die "cannot close readiness pipe: $!\n";
        $args{child_setup}->() if $args{child_setup};
        exec { $command[0] } @command;
        die "cannot exec $command[0]: $!\n";
    }
    close $ready_writer;

    my ($group_ready, $readiness_closed, $deadline_hit, $stdout_oversized, $stderr_oversized) =
        (0, 0, 0, 0, 0);
    my ($term_sent, $kill_sent, $reaped, $drain_expired) = (0, 0, 0, 0);
    my ($kill_direct_attempted, $kill_group_attempted) = (0, 0);
    my ($term_at, $drain_deadline, $wait_status);
    while (1) {
        unless ($group_ready) {
            my $byte = '';
            my $count = sysread($ready_reader, $byte, 1);
            if (defined $count && $count == 1) {
                $byte eq 'R' or fail('readiness pipe emitted an invalid byte');
                $group_ready = 1;
            } elsif (defined $count && $count == 0) {
                $readiness_closed = 1;
            } elsif (!defined $count && $! != EAGAIN && $! != EWOULDBLOCK) {
                fail("cannot read process-group readiness: $!");
            }
        }

        my $leader = linux_process_stat($pid);
        defined $leader or fail("supervised leader disappeared before safe reap: $pid");
        my $leader_zombie = $leader->{state} eq 'Z' || $leader->{state} eq 'X';
        if ($group_ready && $leader->{pgrp} != $pid) {
            fail("supervised leader escaped its confirmed process group: $pid");
        }
        my @live_group_members = $group_ready && $leader_zombie
            ? live_process_group_members($pid) : ();
        my $tree_quiescent = $leader_zombie
            && ($group_ready ? @live_group_members == 0 : $readiness_closed);

        my $stdout_bytes = file_size($args{stdout_path});
        my $stderr_bytes = file_size($args{stderr_path});
        $stdout_oversized = 1 if $stdout_bytes > $stdout_limit;
        $stderr_oversized = 1 if $stderr_bytes > $stderr_limit;
        my $now = clock_gettime(CLOCK_MONOTONIC);
        $deadline_hit = 1 if $now - $started >= $timeout_seconds;

        if (!$term_sent && ($deadline_hit || $stdout_oversized || $stderr_oversized)) {
            kill 'TERM', $pid unless $leader_zombie;
            kill 'TERM', -$pid if $group_ready;
            $term_sent = 1;
            $term_at = $now;
        }
        if ($term_sent && !$kill_sent && $now - $term_at >= $term_grace_us / 1_000_000) {
            if (!$leader_zombie) {
                kill 'KILL', $pid;
                $kill_direct_attempted = 1;
            }
            if ($group_ready) {
                kill 'KILL', -$pid;
                $kill_group_attempted = 1;
            }
            $kill_sent = 1;
            $drain_deadline = $now + $drain_grace_us / 1_000_000;
        }

        if ($tree_quiescent && (!$term_sent || $kill_sent)) {
            my $waited = waitpid($pid, WNOHANG);
            $waited == $pid or fail("safe zombie reap failed for supervised process: $!");
            $wait_status = $?;
            $reaped = 1;
            last;
        }
        if ($kill_sent && $now >= $drain_deadline) {
            $drain_expired = 1;
            last;
        }
        usleep(10_000);
    }
    my $finished_at = clock_gettime(CLOCK_MONOTONIC);
    close $ready_reader or fail("cannot close readiness pipe: $!");
    assert_expected_binary($args{binary}, $args{binary_identity});

    my $final_stdout_bytes = file_size($args{stdout_path});
    my $final_stderr_bytes = file_size($args{stderr_path});
    $stdout_oversized = 1 if $final_stdout_bytes > $stdout_limit;
    $stderr_oversized = 1 if $final_stderr_bytes > $stderr_limit;

    return {
        pid => $pid,
        wait_status => $wait_status,
        group_ready => $group_ready,
        deadline_hit => $deadline_hit,
        stdout_oversized => $stdout_oversized,
        stderr_oversized => $stderr_oversized,
        term_sent => $term_sent,
        kill_sent => $kill_sent,
        kill_direct_attempted => $kill_direct_attempted,
        kill_group_attempted => $kill_group_attempted,
        drain_expired => $drain_expired,
        elapsed_us => int(($finished_at - $started) * 1_000_000 + 0.999999),
        stdout_bytes => $final_stdout_bytes,
        stderr_bytes => $final_stderr_bytes,
    };
}

sub run_one_probe {
    my (%args) = @_;
    my $workload = $args{workload_config}
        // $WORKLOAD{$args{workload}}
        // fail("unknown workload $args{workload}");
    my $mode = $args{mode};
    $mode eq 'off' || $mode eq 'candidate-b' or fail("unknown mode $mode");
    my $prefix = File::Spec->catfile($args{run_dir}, sprintf('probe-%02d-%s-%s',
        $args{launch_ordinal}, $args{workload}, $mode));
    my $stdout_path = "$prefix.stdout";
    my $stderr_path = "$prefix.stderr";
    my $time_path = "$prefix.time";
    my $meta_path = "$prefix.meta";
    my @command = ('/usr/bin/time', '-f',
        'typokat-wu0d-time-v1 exit=%x elapsed_seconds=%e peak_rss_kib=%M',
        '-o', $time_path, '--', $args{binary}, '--ignored', '--exact',
        $workload->{probe}, '--nocapture');
    write_bytes_exclusive("$prefix.command", command_text(@command) . "\n");
    my $warm = validate_and_warm_runtime_inputs(
        $args{root}, $args{binary}, $args{binary_identity});

    my $supervised = supervise_process(
        binary => $args{binary},
        binary_identity => $args{binary_identity},
        stdout_path => $stdout_path,
        stderr_path => $stderr_path,
        command => \@command,
        child_setup => sub {
        for my $key (keys %ENV) {
            delete $ENV{$key} if $key =~ /\ATYPOKAT_WU0D_/;
        }
        $ENV{TYPOKAT_WU0D_CANDIDATE} = $CANDIDATE_VALUE if $mode eq 'candidate-b';
        if (defined $args{evidence_path}) {
            $ENV{TYPOKAT_WU0D_RELEASE_EVIDENCE_PATH} = $args{evidence_path};
        }
        },
    );

    my $pid = $supervised->{pid};
    my $monotonic_us = $supervised->{elapsed_us};
    my $wrapper_exit = defined $supervised->{wait_status}
        ? decode_wait_status($supervised->{wait_status}) : 255;
    my ($stdout, $stdout_read_oversized) = read_bounded_file($stdout_path, $MAX_STDOUT_BYTES);
    my ($stderr, $stderr_read_oversized) = read_bounded_file($stderr_path, $MAX_STDERR_BYTES);
    my ($time, $time_oversized) = read_bounded_file($time_path, $MAX_TIME_BYTES);
    $supervised->{stdout_oversized} ||= $stdout_read_oversized;
    $supervised->{stderr_oversized} ||= $stderr_read_oversized;
    $supervised->{stdout_bytes} = file_size($stdout_path);
    $supervised->{stderr_bytes} = file_size($stderr_path);
    my $time_bytes = file_size($time_path);
    $supervised->{stdout_oversized} = 1
        if $supervised->{stdout_bytes} > $MAX_STDOUT_BYTES;
    $supervised->{stderr_oversized} = 1
        if $supervised->{stderr_bytes} > $MAX_STDERR_BYTES;
    $time_oversized = 1 if $time_bytes > $MAX_TIME_BYTES;

    my ($exit, $elapsed_seconds, $rss_kib) = $time =~
        /\Atypokat-wu0d-time-v1 exit=([0-9]+) elapsed_seconds=([0-9]+(?:\.[0-9]+)?) peak_rss_kib=([0-9]+)\n?\z/;
    my @errors;
    push @errors, 'deadline-hit' if $supervised->{deadline_hit};
    push @errors, 'stdout-oversized' if $supervised->{stdout_oversized};
    push @errors, 'stderr-oversized' if $supervised->{stderr_oversized};
    push @errors, 'time-output-oversized' if $time_oversized;
    push @errors, 'bounded-drain-expired' if $supervised->{drain_expired};
    push @errors, 'wrapper-not-reaped' unless defined $supervised->{wait_status};
    push @errors, 'time-record-missing-or-malformed' unless defined $exit;
    push @errors, "wrapper-exit-$wrapper_exit" if $wrapper_exit != 0;
    if (defined $exit) {
        push @errors, 'time-wrapper-exit-mismatch' if $exit != $wrapper_exit;
        push @errors, "probe-exit-$exit" if $exit != 0;
        my $time_us = decimal_seconds_to_us($elapsed_seconds);
        push @errors, 'elapsed-over-5s' if $time_us > 5_000_000 || $monotonic_us > 5_000_000;
        push @errors, 'rss-over-512mib' if $rss_kib * 1024 > 512 * 1024 * 1024;
    }
    unless (defined $args{evidence_path}) {
        my @summaries = grep { /^typokat-wu0d-candidate-v1(?: |\z)/ } split /\n/, $stdout;
        push @errors, 'summary-count-not-one' unless @summaries == 1;
        if (@summaries == 1) {
            my $expected = "typokat-wu0d-candidate-v1 workload=$args{workload} mode=$mode ";
            push @errors, 'summary-workload-or-mode-mismatch'
                unless index($summaries[0], $expected) == 0;
        }
    }

    my $meta = join "\n",
        'typokat-wu0d-process-meta-v1',
        "workload=$args{workload}",
        "mode=$mode",
        "launch_ordinal=$args{launch_ordinal}",
        "probe_filter=$workload->{probe}",
        "pid=$pid",
        "group_ready=$supervised->{group_ready}",
        "deadline_hit=$supervised->{deadline_hit}",
        "term_sent=$supervised->{term_sent}",
        "kill_sent=$supervised->{kill_sent}",
        "kill_direct_attempted=$supervised->{kill_direct_attempted}",
        "kill_group_attempted=$supervised->{kill_group_attempted}",
        "drain_expired=$supervised->{drain_expired}",
        "wrapper_exit=$wrapper_exit",
        "monotonic_elapsed_us=$monotonic_us",
        'time_elapsed_us=' . (defined $elapsed_seconds
            ? decimal_seconds_to_us($elapsed_seconds) : 'unavailable'),
        "stdout_bytes=$supervised->{stdout_bytes}",
        "stderr_bytes=$supervised->{stderr_bytes}",
        "time_bytes=$time_bytes",
        "warm_regular_files=$warm->{regular_files}",
        "warm_bytes=$warm->{bytes}",
        'errors=' . (@errors ? join(',', @errors) : 'none'),
        '';
    write_bytes_exclusive($meta_path, $meta);
    @errors and fail("probe failed closed (" . join(',', @errors) . "); artifacts: $args{run_dir}");
    return {
        exit_code => 0 + $exit,
        elapsed_us => $monotonic_us,
        peak_rss_bytes => $rss_kib * 1024,
        stdout => $stdout,
        stderr => $stderr,
        probe_filter => $workload->{probe},
        profile_identity => $workload->{profile},
        process_pid => $pid,
    };
}

sub process_identity {
    my (%args) = @_;
    return sha256_hex(join "\0",
        'typokat-wu0d-process-v1',
        $args{binary_identity},
        $args{host_identity},
        $args{run_dir},
        $args{set},
        $args{global_ordinal},
        $args{process_pid},
        $$,
        sprintf('%.9f', clock_gettime(CLOCK_MONOTONIC)),
    );
}

sub evidence_process_line {
    my (%args) = @_;
    my $observation = $args{observation};
    my $identity = $args{process_identity};
    my $probe = $observation->{probe_filter};
    my $stdout = $observation->{stdout};
    return join ' ',
        'process',
        "set=$args{set}",
        "pair=$args{pair}",
        "variant=$args{variant}",
        'process_identity_len=' . length($identity),
        'process_identity_hex=' . lower_hex($identity),
        "launch_ordinal=$args{launch_ordinal}",
        'probe_filter_len=' . length($probe),
        'probe_filter_hex=' . lower_hex($probe),
        "binary_identity=$args{binary_identity}",
        "host_identity=$args{host_identity}",
        "profile_identity=$observation->{profile_identity}",
        'warm_filesystem_cache=1',
        'release_libtest=1',
        "exit_code=$observation->{exit_code}",
        "elapsed_us=$observation->{elapsed_us}",
        "peak_rss_bytes=$observation->{peak_rss_bytes}",
        'stdout_len=' . length($stdout),
        'stdout_hex=' . lower_hex($stdout);
}

sub write_evidence_atomic {
    my ($path, $artifact) = @_;
    length($artifact) <= 4 * 1024 * 1024
        or fail('canonical release evidence exceeds 4 MiB');
    $artifact =~ /\A[\x00-\x7f]+\z/ && $artifact !~ /\r/ && $artifact =~ /\n\z/
        or fail('canonical release evidence is not final-LF ASCII without CR');
    my $temporary = "$path.tmp-$$";
    sysopen my $handle, $temporary, O_WRONLY | O_CREAT | O_EXCL, 0600
        or fail("cannot create evidence temporary $temporary: $!");
    binmode $handle, ':raw';
    print {$handle} $artifact or fail("cannot write evidence temporary $temporary: $!");
    $handle->sync or fail("cannot sync evidence temporary $temporary: $!");
    close $handle or fail("cannot close evidence temporary $temporary: $!");
    chmod 0400, $temporary or fail("cannot chmod evidence temporary $temporary: $!");
    rename $temporary, $path or fail("cannot atomically publish evidence $path: $!");
    -f $path && !-l $path or fail("published evidence is unsafe: $path");
}

sub validate_with_same_binary {
    my (%args) = @_;
    my $probe =
        'check::checker::wu0d_candidate_release_spec::wu0d_candidate_release_validate_evidence_file';
    my $observation = run_one_probe(
        run_dir => $args{run_dir},
        root => $args{root},
        binary => $args{binary},
        binary_identity => $args{binary_identity},
        workload => 'release-validator',
        workload_config => { probe => $probe, profile => '' },
        mode => 'off',
        launch_ordinal => 31,
        evidence_path => $args{evidence_path},
    );
    my @decision = grep { /^typokat-wu0d-release-validation-v1 / }
        split /\n/, $observation->{stdout};
    @decision == 1 or fail("same-binary validator emitted no unique decision; artifacts: $args{run_dir}");
    $decision[0] eq 'typokat-wu0d-release-validation-v1 decision=go reasons=none'
        or fail("same-binary validator rejected evidence: $decision[0]");
    return $decision[0];
}

sub interleaved_schedule {
    return (
        [1, 'off'], [1, 'candidate-b'],
        [2, 'candidate-b'], [2, 'off'],
        [3, 'off'], [3, 'candidate-b'],
        [4, 'candidate-b'], [4, 'off'],
        [5, 'off'], [5, 'candidate-b'],
    );
}

sub run_full_schedule {
    my (%args) = @_;
    my @schedule = interleaved_schedule();
    my @lines = ('typokat-wu0d-release-evidence-v1 process_count=30');
    my %identities;
    my $global_ordinal = 0;
    for my $set ('primary', 'non-cycle', 'reporter-control') {
        for my $index (0 .. $#schedule) {
            my ($pair, $mode) = @{ $schedule[$index] };
            my $launch_ordinal = $index + 1;
            ++$global_ordinal;
            my $observation = run_one_probe(
                run_dir => $args{run_dir},
                root => $args{root},
                binary => $args{binary},
                binary_identity => $args{binary_identity},
                workload => $set,
                mode => $mode,
                launch_ordinal => $launch_ordinal,
            );
            my $identity = process_identity(
                binary_identity => $args{binary_identity},
                host_identity => $args{host_identity},
                run_dir => $args{run_dir},
                set => $set,
                global_ordinal => $global_ordinal,
                process_pid => $observation->{process_pid},
            );
            !$identities{$identity}++ or fail('process identity collision');
            push @lines, evidence_process_line(
                set => $set,
                pair => $pair,
                variant => $mode eq 'off' ? 'off' : 'candidate-b',
                process_identity => $identity,
                launch_ordinal => $launch_ordinal,
                binary_identity => $args{binary_identity},
                host_identity => $args{host_identity},
                observation => $observation,
            );
        }
    }
    $global_ordinal == 30 && scalar(keys %identities) == 30
        or fail('full release schedule did not produce 30 global identities');
    my $artifact = join("\n", @lines) . "\n";
    my $evidence_path = File::Spec->catfile($args{run_dir}, 'release-evidence-v1.txt');
    write_evidence_atomic($evidence_path, $artifact);
    my $decision = validate_with_same_binary(
        run_dir => $args{run_dir},
        root => $args{root},
        binary => $args{binary},
        binary_identity => $args{binary_identity},
        evidence_path => $evidence_path,
    );
    return ($evidence_path, $decision);
}

sub self_test_path {
    my ($directory, $name) = @_;
    return File::Spec->catfile($directory, $name);
}

sub self_test_supervise {
    my (%args) = @_;
    return supervise_process(
        binary => $args{binary},
        binary_identity => $args{binary_identity},
        stdout_path => self_test_path($args{directory}, "$args{name}.stdout"),
        stderr_path => self_test_path($args{directory}, "$args{name}.stderr"),
        command => $args{command},
        timeout_seconds => $args{timeout_seconds} // 0.08,
        term_grace_us => $args{term_grace_us} // 40_000,
        drain_grace_us => 100_000,
        stdout_limit => $args{stdout_limit} // 16 * 1024,
        stderr_limit => $args{stderr_limit} // 16 * 1024,
        pre_setsid_delay_us => $args{pre_setsid_delay_us},
        pre_setsid_setup => $args{pre_setsid_setup},
    );
}

sub live_non_zombie_process {
    my ($pid) = @_;
    my $path = "/proc/$pid/stat";
    return 0 unless -f $path;
    my $stat = read_bytes($path);
    my ($state) = $stat =~ /\) ([A-Z]) /;
    return defined $state && $state ne 'Z';
}

sub adversarial_supervisor_self_test {
    my $root = repo_root();
    my $base = File::Spec->catdir($root, 'target', 'wu0d-release', 'self-tests');
    make_path($base, { mode => 0700 });
    my $directory = File::Spec->catdir($base,
        sprintf('%d-%d', $$, int(clock_gettime(CLOCK_MONOTONIC) * 1_000_000)));
    mkdir $directory, 0700 or fail("cannot create self-test directory $directory: $!");
    my $perl = '/usr/bin/perl';
    my $perl_identity = sha256_hex(read_bytes($perl));

    my $delayed = self_test_supervise(
        directory => $directory,
        name => 'delayed-pre-setsid',
        binary => $perl,
        binary_identity => $perl_identity,
        command => [$perl, '-e', 'exit 0'],
        timeout_seconds => 0.04,
        term_grace_us => 100_000,
        pre_setsid_delay_us => 150_000,
    );
    $delayed->{deadline_hit} && !$delayed->{group_ready}
        && $delayed->{term_sent} && $delayed->{kill_sent}
        && !$delayed->{kill_direct_attempted} && !$delayed->{kill_group_attempted}
        or fail("pre-setsid timeout self-test failed; artifacts: $directory");

    my $delayed_ignoring = self_test_supervise(
        directory => $directory,
        name => 'delayed-pre-setsid-term-ignoring',
        binary => $perl,
        binary_identity => $perl_identity,
        command => [$perl, '-e', 'exit 0'],
        timeout_seconds => 0.04,
        pre_setsid_delay_us => 300_000,
        pre_setsid_setup => sub { $SIG{TERM} = 'IGNORE' },
    );
    $delayed_ignoring->{deadline_hit} && !$delayed_ignoring->{group_ready}
        && $delayed_ignoring->{term_sent} && $delayed_ignoring->{kill_sent}
        && $delayed_ignoring->{kill_direct_attempted}
        && !$delayed_ignoring->{kill_group_attempted}
        && defined $delayed_ignoring->{wait_status}
        && decode_wait_status($delayed_ignoring->{wait_status}) == 137
        or fail("pre-setsid direct KILL self-test failed; artifacts: $directory");

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
        directory => $directory,
        name => 'term-ignoring-descendant',
        binary => $perl,
        binary_identity => $perl_identity,
        command => [$perl, '-e', $descendant_program],
    );
    $descendant->{group_ready} && $descendant->{deadline_hit}
        && $descendant->{term_sent} && $descendant->{kill_sent}
        && !$descendant->{kill_direct_attempted} && $descendant->{kill_group_attempted}
        && defined $descendant->{wait_status}
        && decode_wait_status($descendant->{wait_status}) == 0
        or fail("descendant kill self-test failed; artifacts: $directory");
    my ($descendant_stdout, $descendant_stdout_oversized) = read_bounded_file(
        self_test_path($directory, 'term-ignoring-descendant.stdout'), 1024);
    !$descendant_stdout_oversized
        or fail("descendant PID output exceeded its self-test bound; artifacts: $directory");
    my ($descendant_pid) = $descendant_stdout =~ /\A([0-9]+)\n\z/;
    defined $descendant_pid or fail("descendant PID capture failed; artifacts: $directory");
    for (1 .. 30) {
        last unless live_non_zombie_process($descendant_pid);
        usleep(10_000);
    }
    !live_non_zombie_process($descendant_pid)
        or fail("TERM-ignoring descendant survived KILL; artifacts: $directory");

    my $flood_program = <<'PERL';
my $child = fork();
die "fork failed: $!\n" unless defined $child;
exit 0 if $child != 0;
$SIG{TERM} = 'IGNORE';
$| = 1;
print "x" x (64 * 1024);
select undef, undef, undef, 10 while 1;
PERL
    my $flood = self_test_supervise(
        directory => $directory,
        name => 'exited-leader-stdout-flood',
        binary => $perl,
        binary_identity => $perl_identity,
        command => [$perl, '-e', $flood_program],
        stdout_limit => 1024,
        timeout_seconds => 1,
    );
    $flood->{stdout_oversized} && $flood->{term_sent} && $flood->{kill_sent}
        && !$flood->{kill_direct_attempted} && $flood->{kill_group_attempted}
        or fail("stdout cap self-test failed; artifacts: $directory");
    $flood->{stdout_bytes} > 1024
        or fail("stdout flood was not preserved; artifacts: $directory");
    my ($flood_stdout, $flood_read_oversized) = read_bounded_file(
        self_test_path($directory, 'exited-leader-stdout-flood.stdout'), 1024);
    $flood_read_oversized && $flood_stdout eq ''
        or fail("stdout flood bypassed bounded post-read; artifacts: $directory");

    my $oversized_time = self_test_path($directory, 'oversized.time');
    write_bytes_exclusive($oversized_time, 't' x ($MAX_TIME_BYTES + 1));
    my ($time_prefix, $time_read_oversized) =
        read_bounded_file($oversized_time, $MAX_TIME_BYTES);
    $time_read_oversized && $time_prefix eq ''
        or fail("time output bypassed bounded post-read; artifacts: $directory");

    my $swap = self_test_path($directory, 'binary-swap.pl');
    write_bytes_exclusive($swap, <<'PERL');
#!/usr/bin/env perl
use strict;
use warnings;
chmod 0700, $0 or die "chmod failed: $!\n";
open my $handle, '>>:raw', $0 or die "append failed: $!\n";
print {$handle} "# swapped\n" or die "write failed: $!\n";
close $handle or die "close failed: $!\n";
PERL
    chmod 0500, $swap or fail("cannot chmod swap self-test binary: $!");
    my $swap_identity = sha256_hex(read_bytes($swap));
    my $swap_error = '';
    eval {
        self_test_supervise(
            directory => $directory,
            name => 'binary-swap',
            binary => $swap,
            binary_identity => $swap_identity,
            command => [$swap],
            timeout_seconds => 1,
        );
        1;
    } or $swap_error = $@;
    $swap_error =~ /frozen libtest digest mismatch/
        or fail("binary swap self-test did not fail closed; artifacts: $directory");

    remove_tree($directory);
    !-e $directory or fail("cannot remove completed self-test directory: $directory");
}

sub provenance_self_test {
    my $root = repo_root();
    my $inventory = strict_profile_inventory($root);
    $inventory->{source_count} == 82 && @{ $inventory->{all_files} } == 88
        or fail('strict profile inventory self-test failed');

    my $rust_path = File::Spec->catfile(
        $root, 'src', 'check', 'checker', 'wu0d_candidate_release.rs');
    my $rust = read_regular_input($rust_path);
    verify_control_fixture_bytes($rust);
    my $drifted = $rust;
    $drifted =~ s/interface Wu0dBox/interface Wu0dDrift/
        or fail('control fixture drift self-test could not mutate the source');
    my $fixture_error = '';
    eval { verify_control_fixture_bytes($drifted); 1 } or $fixture_error = $@;
    $fixture_error =~ /non-cycle Rust fixture identity changed/
        or fail('control fixture drift did not fail closed');

    my $reporter_drift = $rust;
    $reporter_drift =~ s/export default function report/export default function drift/
        or fail('reporter fixture drift self-test could not mutate the source');
    my $reporter_error = '';
    eval { verify_control_fixture_bytes($reporter_drift); 1 } or $reporter_error = $@;
    $reporter_error =~ /reporter-control Rust fixture identity changed/
        or fail('reporter fixture drift did not fail closed');

    my $inventory_error = '';
    eval { assert_exact_inventory_names(['lib', 'unexpected'], ['lib'], 'self-test'); 1 }
        or $inventory_error = $@;
    $inventory_error =~ /profile directory inventory changed/
        or fail('profile inventory drift did not fail closed');
}

sub self_test {
    decimal_seconds_to_us('0') == 0 or fail('elapsed conversion failed');
    decimal_seconds_to_us('1.000001') == 1_000_001 or fail('elapsed conversion failed');
    decimal_seconds_to_us('0.000000001') == 1 or fail('elapsed ceiling failed');
    scalar(keys %WORKLOAD) == 3 or fail('workload table changed');
    $WORKLOAD{primary}{profile} eq $PROFILE_IDENTITY or fail('primary profile changed');
    my @schedule = interleaved_schedule();
    join(',', map { "$_->[0]:$_->[1]" } @schedule) eq
        '1:off,1:candidate-b,2:candidate-b,2:off,3:off,3:candidate-b,4:candidate-b,4:off,5:off,5:candidate-b'
        or fail('interleaved schedule changed');
    lower_hex("\x00\xff") eq '00ff' or fail('lowercase hex encoding failed');
    provenance_self_test();
    adversarial_supervisor_self_test();
    print "typokat-wu0d-runner-self-test-v1 result=ok\n";
}

sub main {
    my @arguments = @ARGV;
    if (@arguments == 1 && $arguments[0] eq '--self-test') {
        self_test();
        return;
    }
    if (@arguments == 1 && ($arguments[0] eq '--help' || $arguments[0] eq '-h')) {
        print usage();
        return;
    }

    my $dry_run = 0;
    @arguments = grep { $_ eq '--dry-run' ? ($dry_run = 1, 0) : 1 } @arguments;
    my ($workload, $mode, $full);
    if (@arguments == 1 && $arguments[0] eq '--smoke-control') {
        ($workload, $mode) = ('non-cycle', 'off');
    } elsif (@arguments == 3 && $arguments[0] eq '--single') {
        ($workload, $mode) = @arguments[1, 2];
    } elsif (@arguments == 1 && $arguments[0] eq '--full') {
        ($workload, $mode, $full) = ('primary', 'off', 1);
    } elsif ($dry_run && @arguments == 0) {
        ($workload, $mode) = ('primary', 'off');
    } else {
        fail("invalid arguments\n" . usage());
    }
    exists $WORKLOAD{$workload} or fail("unknown workload $workload");
    $mode eq 'off' || $mode eq 'candidate-b' or fail("unknown mode $mode");

    my $root = repo_root();
    chdir $root or fail("cannot chdir to $root: $!");
    verify_control_fixtures($root);
    my $inventory = strict_profile_inventory($root);
    if ($dry_run) {
        my $process_count = $full ? 30 : 1;
        print "typokat-wu0d-runner-dry-v1 workload=$workload mode=$mode profile_files=",
            $inventory->{source_count},
            " build_count=1 process_count=$process_count timeout_seconds=$TIMEOUT_SECONDS\n";
        return;
    }
    -x '/usr/bin/time' or fail('/usr/bin/time is required');
    my $run_dir = create_run_directory($root);
    my $built = build_release_libtest_once($root, $run_dir);
    my ($binary, $binary_digest) = freeze_libtest($root, $built);
    my ($host_facts, $host_digest) = host_facts();
    write_bytes_exclusive(File::Spec->catfile($run_dir, 'host-facts.txt'), $host_facts);
    my $facts = join "\n",
        'typokat-wu0d-run-facts-v1',
        "binary=$binary",
        "binary_identity=$binary_digest",
        "host_identity=$host_digest",
        "primary_profile_identity=$PROFILE_IDENTITY",
        'profile_files=82',
        'profile_source_bytes=2936611',
        '';
    write_bytes_exclusive(File::Spec->catfile($run_dir, 'run-facts.txt'), $facts);
    if ($full) {
        my ($evidence_path, $decision) = run_full_schedule(
            run_dir => $run_dir,
            root => $root,
            binary => $binary,
            binary_identity => $binary_digest,
            host_identity => $host_digest,
        );
        print "typokat-wu0d-runner-v1 result=ok mode=full process_count=30 ",
            "binary_identity=$binary_digest host_identity=$host_digest ",
            "decision=go evidence=$evidence_path artifacts=$run_dir\n";
        return;
    }
    my $observation = run_one_probe(
        run_dir => $run_dir,
        root => $root,
        binary => $binary,
        binary_identity => $binary_digest,
        workload => $workload,
        mode => $mode,
        launch_ordinal => 1,
    );
    print "typokat-wu0d-runner-v1 result=ok workload=$workload mode=$mode ",
        "elapsed_us=$observation->{elapsed_us} peak_rss_bytes=$observation->{peak_rss_bytes} ",
        "binary_identity=$binary_digest host_identity=$host_digest artifacts=$run_dir\n";
}

main();
