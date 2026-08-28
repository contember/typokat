package main

import (
	"bufio"
	"context"
	"crypto/sha256"
	_ "embed"
	"encoding/binary"
	"encoding/hex"
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"time"

	"github.com/microsoft/typescript-go/internal/ast"
	"github.com/microsoft/typescript-go/internal/compiler"
	"github.com/microsoft/typescript-go/internal/core"
	"github.com/microsoft/typescript-go/internal/tsoptions"
	"github.com/microsoft/typescript-go/internal/tspath"
	"github.com/microsoft/typescript-go/internal/vfs"
	"github.com/microsoft/typescript-go/internal/vfs/vfstest"
	"golang.org/x/sys/unix"
)

const (
	protocolSchema   = 1
	upstreamRevision = "2bd066d87f5bafd315be9f40889d0a60b9e58e0b"
	profileIdentity  = "ea59b3e150195f6cfe843661c0bcb006cffb04dd988861778a188be9441c579d"
	userFileName     = "/typokat-wu6/frame.ts"
	workDirectory    = "/typokat-wu6"
)

var compilerFlags = []string{
	"--strict", "--target", "es2025", "--module", "preserve", "--noEmit", "--pretty", "false", "--ignoreConfig",
}

var profileNames = []string{
	"lib.es5.d.ts",
	"lib.es2015.d.ts",
	"lib.es2016.d.ts",
	"lib.es2017.d.ts",
	"lib.es2018.d.ts",
	"lib.es2019.d.ts",
	"lib.es2020.d.ts",
	"lib.es2021.d.ts",
	"lib.es2022.d.ts",
	"lib.es2023.d.ts",
	"lib.es2024.d.ts",
	"lib.es2025.d.ts",
	"lib.dom.d.ts",
	"lib.dom.iterable.d.ts",
	"lib.dom.asynciterable.d.ts",
	"lib.webworker.importscripts.d.ts",
	"lib.scripthost.d.ts",
	"lib.es2015.core.d.ts",
	"lib.es2015.collection.d.ts",
	"lib.es2015.generator.d.ts",
	"lib.es2015.iterable.d.ts",
	"lib.es2015.promise.d.ts",
	"lib.es2015.proxy.d.ts",
	"lib.es2015.reflect.d.ts",
	"lib.es2015.symbol.d.ts",
	"lib.es2015.symbol.wellknown.d.ts",
	"lib.es2016.array.include.d.ts",
	"lib.es2016.intl.d.ts",
	"lib.es2017.arraybuffer.d.ts",
	"lib.es2017.date.d.ts",
	"lib.es2017.object.d.ts",
	"lib.es2017.sharedmemory.d.ts",
	"lib.es2017.string.d.ts",
	"lib.es2017.intl.d.ts",
	"lib.es2017.typedarrays.d.ts",
	"lib.es2018.asyncgenerator.d.ts",
	"lib.es2018.asynciterable.d.ts",
	"lib.es2018.intl.d.ts",
	"lib.es2018.promise.d.ts",
	"lib.es2018.regexp.d.ts",
	"lib.es2019.array.d.ts",
	"lib.es2019.object.d.ts",
	"lib.es2019.string.d.ts",
	"lib.es2019.symbol.d.ts",
	"lib.es2019.intl.d.ts",
	"lib.es2020.bigint.d.ts",
	"lib.es2020.date.d.ts",
	"lib.es2020.promise.d.ts",
	"lib.es2020.sharedmemory.d.ts",
	"lib.es2020.string.d.ts",
	"lib.es2020.symbol.wellknown.d.ts",
	"lib.es2020.intl.d.ts",
	"lib.es2020.number.d.ts",
	"lib.es2021.promise.d.ts",
	"lib.es2021.string.d.ts",
	"lib.es2021.weakref.d.ts",
	"lib.es2021.intl.d.ts",
	"lib.es2022.array.d.ts",
	"lib.es2022.error.d.ts",
	"lib.es2022.intl.d.ts",
	"lib.es2022.object.d.ts",
	"lib.es2022.string.d.ts",
	"lib.es2022.regexp.d.ts",
	"lib.es2023.array.d.ts",
	"lib.es2023.collection.d.ts",
	"lib.es2023.intl.d.ts",
	"lib.es2024.arraybuffer.d.ts",
	"lib.es2024.collection.d.ts",
	"lib.es2024.object.d.ts",
	"lib.es2024.promise.d.ts",
	"lib.es2024.regexp.d.ts",
	"lib.es2024.sharedmemory.d.ts",
	"lib.es2024.string.d.ts",
	"lib.es2025.collection.d.ts",
	"lib.es2025.float16.d.ts",
	"lib.es2025.intl.d.ts",
	"lib.es2025.iterator.d.ts",
	"lib.es2025.promise.d.ts",
	"lib.es2025.regexp.d.ts",
	"lib.decorators.d.ts",
	"lib.decorators.legacy.d.ts",
	"lib.es2025.full.d.ts",
}

//go:embed main.go
var runnerSource []byte

type protocolInfo struct {
	Schema            int      `json:"schema"`
	Tool              string   `json:"tool"`
	SourceSHA256      string   `json:"source_sha256"`
	BinarySHA256      string   `json:"binary_sha256"`
	UpstreamRevision  string   `json:"upstream_revision"`
	ProfileSHA256     string   `json:"profile_sha256"`
	ProfileFileCount  int      `json:"profile_file_count"`
	ProgramPerFrame   bool     `json:"program_per_frame"`
	CheckerPerFrame   bool     `json:"checker_per_frame"`
	SharedUserSources bool     `json:"shared_user_sources"`
	Incremental       bool     `json:"incremental"`
	SingleThreaded    bool     `json:"single_threaded"`
	CompilerFlags     []string `json:"compiler_flags"`
}

type frameRequest struct {
	Schema int    `json:"schema"`
	CaseID string `json:"case_id"`
	Source string `json:"source"`
}

type faultRequest struct {
	Schema int    `json:"schema"`
	Fault  string `json:"fault"`
}

type interval struct {
	StartNS int64 `json:"start_ns"`
	EndNS   int64 `json:"end_ns"`
}

type framePhases struct {
	ProductInitialization interval `json:"product_initialization"`
	ParsePrepare          interval `json:"parse_prepare"`
	Binding               interval `json:"binding"`
	PublicationCheck      interval `json:"publication_check"`
	Release               interval `json:"release"`
}

type semanticResult struct {
	ExitCode    int      `json:"exit_code"`
	ParseErrors []string `json:"parse_errors"`
	Diagnostics []string `json:"diagnostics"`
	Incompletes []string `json:"incompletes"`
}

type frameReceipt struct {
	Ordinal                int            `json:"ordinal"`
	CaseID                 string         `json:"case_id"`
	LifecycleDisposition   string         `json:"lifecycle_disposition"`
	Semantic               semanticResult `json:"semantic"`
	RouteStartNS           int64          `json:"route_start_ns"`
	RouteEndNS             int64          `json:"route_end_ns"`
	ProductInitializations int            `json:"product_initializations"`
	Phases                 framePhases    `json:"phases"`
	ProgramIdentity        string         `json:"program_identity"`
	CheckerIdentity        string         `json:"checker_identity"`
	SemanticIdentity       string         `json:"semantic_identity"`
	SharedProductIdentity  string         `json:"shared_product_identity"`
	RetainedFrameState     int            `json:"retained_frame_state"`
	RetainedFrameHandles   int            `json:"retained_frame_handles"`
}

type sharedLibraries struct {
	libDirectory string
	fs           vfs.FS
	files        map[tspath.Path]*ast.SourceFile
	sealed       bool
}

type libraryCachingHost struct {
	compiler.CompilerHost
	libraries *sharedLibraries
}

func (h *libraryCachingHost) GetSourceFile(opts ast.SourceFileParseOptions) *ast.SourceFile {
	if !strings.HasPrefix(opts.FileName, h.libraries.libDirectory+"/") {
		return h.CompilerHost.GetSourceFile(opts)
	}
	if cached, ok := h.libraries.files[opts.Path]; ok {
		if cached.ParseOptions() != opts {
			panic("cached library parse options changed")
		}
		return cached
	}
	if h.libraries.sealed {
		panic("program requested an unauthenticated library after product initialization")
	}
	file := h.CompilerHost.GetSourceFile(opts)
	if file != nil {
		h.libraries.files[opts.Path] = file
	}
	return file
}

type frameFS struct {
	base       vfs.FS
	userSource string
}

func (f *frameFS) UseCaseSensitiveFileNames() bool { return true }
func (f *frameFS) FileExists(path string) bool {
	return path == userFileName || f.base.FileExists(path)
}
func (f *frameFS) ReadFile(path string) (string, bool) {
	if path == userFileName {
		return f.userSource, true
	}
	return f.base.ReadFile(path)
}
func (f *frameFS) WriteFile(string, string) error             { return vfs.ErrPermission }
func (f *frameFS) AppendFile(string, string) error            { return vfs.ErrPermission }
func (f *frameFS) Remove(string) error                        { return vfs.ErrPermission }
func (f *frameFS) Chtimes(string, time.Time, time.Time) error { return vfs.ErrPermission }
func (f *frameFS) DirectoryExists(path string) bool {
	return path == workDirectory || f.base.DirectoryExists(path)
}
func (f *frameFS) GetAccessibleEntries(path string) vfs.Entries {
	if path == workDirectory {
		return vfs.Entries{Files: []string{"frame.ts"}}
	}
	return f.base.GetAccessibleEntries(path)
}
func (f *frameFS) Stat(path string) vfs.FileInfo {
	if path == userFileName {
		return staticFileInfo{name: "frame.ts", size: int64(len(f.userSource))}
	}
	if path == workDirectory {
		return staticFileInfo{name: "typokat-wu6", directory: true}
	}
	return f.base.Stat(path)
}
func (f *frameFS) WalkDir(root string, walkFn vfs.WalkDirFunc) error {
	if root != workDirectory && root != userFileName {
		return f.base.WalkDir(root, walkFn)
	}
	directory := staticFileInfo{name: "typokat-wu6", directory: true}
	if err := walkFn(workDirectory, fs.FileInfoToDirEntry(directory), nil); err != nil {
		return err
	}
	if root == userFileName {
		return walkFn(userFileName, fs.FileInfoToDirEntry(f.Stat(userFileName)), nil)
	}
	return walkFn(userFileName, fs.FileInfoToDirEntry(f.Stat(userFileName)), nil)
}
func (f *frameFS) Realpath(path string) string { return path }

type staticFileInfo struct {
	name      string
	size      int64
	directory bool
}

func (i staticFileInfo) Name() string { return i.name }
func (i staticFileInfo) Size() int64  { return i.size }
func (i staticFileInfo) Mode() fs.FileMode {
	if i.directory {
		return fs.ModeDir | 0o555
	}
	return 0o444
}
func (i staticFileInfo) ModTime() time.Time { return time.Time{} }
func (i staticFileInfo) IsDir() bool        { return i.directory }
func (i staticFileInfo) Sys() any           { return nil }

type runner struct {
	libraries    *sharedLibraries
	nextIdentity uint64
}

func main() {
	os.Exit(run())
}

func run() int {
	defaultLibDirectory, err := executableDirectory()
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		return 2
	}
	flags := flag.NewFlagSet(os.Args[0], flag.ContinueOnError)
	flags.SetOutput(os.Stderr)
	protocol := flags.Bool("protocol-info", false, "print authenticated protocol metadata")
	libDirectory := flags.String("lib-dir", defaultLibDirectory, "directory containing the frozen TypeScript 6.0.3 libraries")
	if err := flags.Parse(os.Args[1:]); err != nil {
		return 2
	}
	if flags.NArg() != 0 {
		fmt.Fprintln(os.Stderr, "unexpected positional argument")
		return 2
	}

	libraries, err := initializeLibraries(*libDirectory)
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		return 2
	}
	if *protocol {
		if err := writeProtocolInfo(libraries); err != nil {
			fmt.Fprintln(os.Stderr, err)
			return 2
		}
		return 0
	}

	runner := runner{libraries: libraries}
	scanner := bufio.NewScanner(os.Stdin)
	scanner.Buffer(make([]byte, 64*1024), 64*1024*1024)
	writer := bufio.NewWriter(os.Stdout)
	ordinal := 0
	for scanner.Scan() {
		line := append([]byte(nil), scanner.Bytes()...)
		kind, err := requestKind(line)
		if err != nil {
			fmt.Fprintln(os.Stderr, err)
			return 2
		}
		if kind == "fault" {
			if err := rejectFault(writer, line); err != nil {
				fmt.Fprintln(os.Stderr, err)
				return 2
			}
			if err := writer.Flush(); err != nil {
				fmt.Fprintln(os.Stderr, err)
			}
			return 2
		}
		request, err := decodeFrameRequest(line)
		if err != nil {
			fmt.Fprintln(os.Stderr, err)
			return 2
		}
		receipt, err := runner.checkFrame(ordinal, request)
		if err != nil {
			fmt.Fprintln(os.Stderr, err)
			return 2
		}
		if err := writeJSON(writer, receipt); err != nil {
			fmt.Fprintln(os.Stderr, err)
			return 2
		}
		ordinal++
	}
	if err := scanner.Err(); err != nil {
		fmt.Fprintln(os.Stderr, err)
		return 2
	}
	if err := writer.Flush(); err != nil {
		fmt.Fprintln(os.Stderr, err)
		return 2
	}
	return 0
}

func initializeLibraries(directory string) (*sharedLibraries, error) {
	absolute, err := filepath.Abs(directory)
	if err != nil {
		return nil, fmt.Errorf("resolve library directory: %w", err)
	}
	absolute = tspath.NormalizeSlashes(absolute)
	contents := make(map[string]string, len(profileNames))
	digest := sha256.New()
	for _, name := range profileNames {
		path := filepath.Join(filepath.FromSlash(absolute), name)
		data, err := os.ReadFile(path)
		if err != nil {
			return nil, fmt.Errorf("read authenticated library %s: %w", name, err)
		}
		var lengths [16]byte
		binary.BigEndian.PutUint64(lengths[:8], uint64(len(name)))
		binary.BigEndian.PutUint64(lengths[8:], uint64(len(data)))
		_, _ = digest.Write(lengths[:])
		_, _ = digest.Write([]byte(name))
		_, _ = digest.Write(data)
		contents[tspath.CombinePaths(absolute, name)] = string(data)
	}
	if observed := hex.EncodeToString(digest.Sum(nil)); observed != profileIdentity {
		return nil, fmt.Errorf("library profile identity differs: %s", observed)
	}
	libraries := &sharedLibraries{
		libDirectory: absolute,
		fs:           vfstest.FromMap(contents, true),
		files:        make(map[tspath.Path]*ast.SourceFile, len(profileNames)),
	}
	warmup := newProgram(libraries, "export {};\n")
	warmup.BindSourceFiles()
	if len(libraries.files) != len(profileNames) {
		return nil, fmt.Errorf("bound library cache has %d files, expected %d", len(libraries.files), len(profileNames))
	}
	for path, file := range libraries.files {
		if file == nil || !file.IsBound() {
			return nil, fmt.Errorf("library source file is not bound: %s", path)
		}
	}
	libraries.sealed = true
	warmup = nil
	return libraries, nil
}

func newProgram(libraries *sharedLibraries, source string) *compiler.Program {
	frameFileSystem := &frameFS{base: libraries.fs, userSource: source}
	baseHost := compiler.NewCompilerHost(workDirectory, frameFileSystem, libraries.libDirectory, nil, nil)
	host := &libraryCachingHost{CompilerHost: baseHost, libraries: libraries}
	options := &core.CompilerOptions{
		Strict:         core.TSTrue,
		Target:         core.ScriptTargetES2025,
		Module:         core.ModuleKindPreserve,
		NoEmit:         core.TSTrue,
		IgnoreConfig:   core.TSTrue,
		SingleThreaded: core.TSTrue,
	}
	return compiler.NewProgram(compiler.ProgramOptions{
		Host: host,
		Config: &tsoptions.ParsedCommandLine{ParsedConfig: &core.ParsedOptions{
			FileNames:       []string{userFileName},
			CompilerOptions: options,
		}},
		SingleThreaded: core.TSTrue,
	})
}

func (r *runner) checkFrame(ordinal int, request frameRequest) (frameReceipt, error) {
	start := monotonicNow()
	initEnd := start
	program := newProgram(r.libraries, request.Source)
	parseEnd := monotonicNow()
	programIdentity := r.identity("program")

	program.BindSourceFiles()
	bindEnd := monotonicNow()
	checkerIdentity := r.identity("checker")
	semanticIdentity := r.identity("semantic")
	checker, releaseChecker := program.GetTypeChecker(context.Background())
	if checker == nil {
		return frameReceipt{}, errors.New("program returned a nil checker")
	}
	releaseChecker()

	all := compiler.GetDiagnosticsOfAnyProgram(
		context.Background(), program, nil, true,
		program.GetBindDiagnostics, program.GetSemanticDiagnostics,
	)
	all = compiler.SortAndDeduplicateDiagnostics(all)
	parseErrors := []string{}
	diagnostics := normalizeDiagnostics(all)
	publicationEnd := monotonicNow()

	program = nil
	checker = nil
	releaseEnd := monotonicNow()
	exitCode := 0
	if len(parseErrors) != 0 || len(diagnostics) != 0 {
		exitCode = 1
	}
	return frameReceipt{
		Ordinal:              ordinal,
		CaseID:               request.CaseID,
		LifecycleDisposition: "semantic",
		Semantic: semanticResult{
			ExitCode: exitCode, ParseErrors: parseErrors, Diagnostics: diagnostics, Incompletes: []string{},
		},
		RouteStartNS:           start,
		RouteEndNS:             releaseEnd,
		ProductInitializations: 0,
		Phases: framePhases{
			ProductInitialization: interval{StartNS: start, EndNS: initEnd},
			ParsePrepare:          interval{StartNS: initEnd, EndNS: parseEnd},
			Binding:               interval{StartNS: parseEnd, EndNS: bindEnd},
			PublicationCheck:      interval{StartNS: bindEnd, EndNS: publicationEnd},
			Release:               interval{StartNS: publicationEnd, EndNS: releaseEnd},
		},
		ProgramIdentity:       programIdentity,
		CheckerIdentity:       checkerIdentity,
		SemanticIdentity:      semanticIdentity,
		SharedProductIdentity: profileIdentity,
		RetainedFrameState:    0,
		RetainedFrameHandles:  0,
	}, nil
}

func (r *runner) identity(kind string) string {
	r.nextIdentity++
	return fmt.Sprintf("%s-%d", kind, r.nextIdentity)
}

func normalizeDiagnostics(diagnostics []*ast.Diagnostic) []string {
	type row struct {
		line int
		code int32
	}
	rows := make([]row, 0, len(diagnostics))
	for _, diagnostic := range diagnostics {
		file := diagnostic.File()
		if file == nil {
			continue
		}
		line, _ := core.PositionToLineAndByteOffset(diagnostic.Pos(), file.ECMALineMap())
		rows = append(rows, row{line: line + 1, code: diagnostic.Code()})
	}
	sort.Slice(rows, func(i, j int) bool {
		if rows[i].line != rows[j].line {
			return rows[i].line < rows[j].line
		}
		return rows[i].code < rows[j].code
	})
	result := make([]string, 0, len(rows))
	for _, row := range rows {
		result = append(result, fmt.Sprintf("%d:%d", row.line, row.code))
	}
	return result
}

func requestKind(data []byte) (string, error) {
	var value map[string]json.RawMessage
	if err := json.Unmarshal(data, &value); err != nil {
		return "", fmt.Errorf("decode request: %w", err)
	}
	if _, ok := value["fault"]; ok {
		return "fault", nil
	}
	return "frame", nil
}

func decodeFrameRequest(data []byte) (frameRequest, error) {
	var fields map[string]json.RawMessage
	if err := json.Unmarshal(data, &fields); err != nil {
		return frameRequest{}, fmt.Errorf("decode frame request: %w", err)
	}
	if len(fields) != 3 || fields["schema"] == nil || fields["case_id"] == nil || fields["source"] == nil {
		return frameRequest{}, errors.New("frame request must contain exactly schema, case_id, and source")
	}
	var request frameRequest
	if err := json.Unmarshal(data, &request); err != nil {
		return frameRequest{}, fmt.Errorf("decode frame request: %w", err)
	}
	if request.Schema != protocolSchema || request.CaseID == "" {
		return frameRequest{}, errors.New("frame request has an invalid schema or empty case_id")
	}
	return request, nil
}

func rejectFault(writer *bufio.Writer, data []byte) error {
	var fields map[string]json.RawMessage
	if err := json.Unmarshal(data, &fields); err != nil {
		return fmt.Errorf("decode fault request: %w", err)
	}
	if len(fields) != 2 || fields["schema"] == nil || fields["fault"] == nil {
		return errors.New("fault request must contain exactly schema and fault")
	}
	var request faultRequest
	if err := json.Unmarshal(data, &request); err != nil {
		return fmt.Errorf("decode fault request: %w", err)
	}
	if request.Schema != protocolSchema || (request.Fault != "reuse-checker" && request.Fault != "retain-frame") {
		return errors.New("unsupported fault request")
	}
	return writeJSON(writer, struct {
		Schema               int    `json:"schema"`
		Fault                string `json:"fault"`
		FaultHookActivations int    `json:"fault_hook_activations"`
		Rejected             bool   `json:"rejected"`
	}{Schema: protocolSchema, Fault: request.Fault, FaultHookActivations: 1, Rejected: true})
}

func writeProtocolInfo(libraries *sharedLibraries) error {
	if len(libraries.files) != len(profileNames) || !libraries.sealed {
		return errors.New("library product is not authenticated and sealed")
	}
	binaryIdentity, err := executableSHA256()
	if err != nil {
		return err
	}
	return writeJSON(bufio.NewWriter(os.Stdout), protocolInfo{
		Schema: protocolSchema, Tool: "tsgo",
		SourceSHA256: sha256Hex(runnerSource), BinarySHA256: binaryIdentity,
		UpstreamRevision: upstreamRevision, ProfileSHA256: profileIdentity, ProfileFileCount: len(profileNames),
		ProgramPerFrame: true, CheckerPerFrame: true, SharedUserSources: false,
		Incremental: false, SingleThreaded: true, CompilerFlags: compilerFlags,
	})
}

func writeJSON(writer *bufio.Writer, value any) error {
	if err := json.NewEncoder(writer).Encode(value); err != nil {
		return err
	}
	return writer.Flush()
}

func executableDirectory() (string, error) {
	path, err := os.Executable()
	if err != nil {
		return "", fmt.Errorf("locate runner executable: %w", err)
	}
	return filepath.Dir(path), nil
}

func executableSHA256() (string, error) {
	path, err := os.Executable()
	if err != nil {
		return "", fmt.Errorf("locate runner executable: %w", err)
	}
	data, err := os.ReadFile(path)
	if err != nil {
		return "", fmt.Errorf("read runner executable: %w", err)
	}
	return sha256Hex(data), nil
}

func sha256Hex(data []byte) string {
	digest := sha256.Sum256(data)
	return hex.EncodeToString(digest[:])
}

func monotonicNow() int64 {
	var value unix.Timespec
	if err := unix.ClockGettime(unix.CLOCK_MONOTONIC, &value); err != nil {
		panic(fmt.Sprintf("read monotonic clock: %v", err))
	}
	return value.Nano()
}

var _ compiler.CompilerHost = (*libraryCachingHost)(nil)
var _ vfs.FS = (*frameFS)(nil)
