package main

import (
	"bytes"
	"encoding/json"
	"sort"
	"strings"
	"testing"

	"github.com/microsoft/typescript-go/internal/ast"
	"github.com/microsoft/typescript-go/internal/tspath"
	"github.com/microsoft/typescript-go/internal/vfs/vfstest"
)

func TestSingleProjectChecksOrderedCrossFileProgram(t *testing.T) {
	t.Parallel()

	libraries := minimalTestLibraries()
	runner := runner{libraries: libraries}
	request := singleProjectRequest{
		Schema: protocolSchema,
		CaseID: "cross-file",
		Files: []singleProjectFile{
			{Path: "pkg/value.ts", Source: "export const value: number = 1;\n"},
			{Path: "pkg/use.ts", Source: "import { value } from \"./value\";\nconst result: string = value;\n"},
		},
	}

	receipt, err := runner.checkSingleProject(request)
	if err != nil {
		t.Fatalf("check single project: %v", err)
	}
	if receipt.Schema != protocolSchema || receipt.Tool != "tsgo" {
		t.Fatalf("unexpected protocol identity: %#v", receipt)
	}
	if receipt.CaseID != request.CaseID || receipt.FileCount != len(request.Files) {
		t.Fatalf("unexpected project identity: %#v", receipt)
	}
	if receipt.Semantic.ExitCode != 1 || len(receipt.Semantic.ParseErrors) != 0 || len(receipt.Semantic.Incompletes) != 0 {
		t.Fatalf("unexpected semantic class: %#v", receipt.Semantic)
	}
	if got, want := receipt.Semantic.Diagnostics, []string{"pkg/use.ts:2:7:2322"}; !equalStrings(got, want) {
		t.Fatalf("diagnostics = %#v, want %#v", got, want)
	}
	if receipt.SharedProductIdentity != profileIdentity {
		t.Fatalf("shared product identity = %q", receipt.SharedProductIdentity)
	}
	identities := []string{receipt.ProgramIdentity, receipt.CheckerIdentity, receipt.SemanticIdentity}
	for _, identity := range identities {
		if identity == "" {
			t.Fatalf("empty local identity in %#v", receipt)
		}
	}
	if receipt.ProgramIdentity == receipt.CheckerIdentity || receipt.ProgramIdentity == receipt.SemanticIdentity || receipt.CheckerIdentity == receipt.SemanticIdentity {
		t.Fatalf("local identities are not distinct: %#v", identities)
	}
	if receipt.RetainedFrameState != 0 || receipt.RetainedFrameHandles != 0 {
		t.Fatalf("single-project state survived teardown: %#v", receipt)
	}
}

func TestDecodeSingleProjectRequestRejectsMalformedFiles(t *testing.T) {
	t.Parallel()

	valid := `{"case_id":"case","files":[{"path":"a.ts","source":"export {};"}],"schema":1}`
	request, err := decodeSingleProjectRequest([]byte(valid))
	if err != nil {
		t.Fatalf("decode valid request: %v", err)
	}
	if request.CaseID != "case" || len(request.Files) != 1 || request.Files[0].Path != "a.ts" {
		t.Fatalf("decoded request differs: %#v", request)
	}

	invalid := map[string]string{
		"top-level extra key": `{"schema":1,"case_id":"case","files":[{"path":"a.ts","source":""}],"extra":false}`,
		"file extra key":      `{"schema":1,"case_id":"case","files":[{"path":"a.ts","source":"","extra":false}]}`,
		"empty case":          `{"schema":1,"case_id":"","files":[{"path":"a.ts","source":""}]}`,
		"empty files":         `{"schema":1,"case_id":"case","files":[]}`,
		"duplicate path":      `{"schema":1,"case_id":"case","files":[{"path":"a.ts","source":""},{"path":"a.ts","source":""}]}`,
		"parent path":         `{"schema":1,"case_id":"case","files":[{"path":"../a.ts","source":""}]}`,
		"absolute path":       `{"schema":1,"case_id":"case","files":[{"path":"/a.ts","source":""}]}`,
		"dot path":            `{"schema":1,"case_id":"case","files":[{"path":"./a.ts","source":""}]}`,
		"backslash path":      `{"schema":1,"case_id":"case","files":[{"path":"dir\\\\a.ts","source":""}]}`,
		"non-TypeScript path": `{"schema":1,"case_id":"case","files":[{"path":"a.js","source":""}]}`,
	}
	for name, raw := range invalid {
		t.Run(name, func(t *testing.T) {
			t.Parallel()
			if _, err := decodeSingleProjectRequest([]byte(raw)); err == nil {
				t.Fatalf("accepted malformed request: %s", raw)
			}
		})
	}
}

func TestSingleProjectModeRequiresExactlyOneRequestAndResponse(t *testing.T) {
	t.Parallel()

	raw := `{"case_id":"one","files":[{"path":"one.ts","source":"export {};"}],"schema":1}`
	for name, input := range map[string]string{
		"empty":        "",
		"multiple":     raw + "\n" + raw + "\n",
		"noncanonical": `{"schema":1,"case_id":"one","files":[{"path":"one.ts","source":"export {};"}]}` + "\n",
	} {
		t.Run(name, func(t *testing.T) {
			t.Parallel()
			var output bytes.Buffer
			err := runSingleProjectMode(strings.NewReader(input), &output, &runner{libraries: minimalTestLibraries()})
			if err == nil {
				t.Fatalf("accepted %s request stream", name)
			}
			if output.Len() != 0 {
				t.Fatalf("%s request stream emitted output: %q", name, output.String())
			}
		})
	}

	var output bytes.Buffer
	if err := runSingleProjectMode(strings.NewReader(raw+"\n"), &output, &runner{libraries: minimalTestLibraries()}); err != nil {
		t.Fatalf("run one request: %v", err)
	}
	lines := strings.Split(strings.TrimSuffix(output.String(), "\n"), "\n")
	if len(lines) != 1 {
		t.Fatalf("response line count = %d, want 1", len(lines))
	}
	var response map[string]json.RawMessage
	if err := json.Unmarshal([]byte(lines[0]), &response); err != nil {
		t.Fatalf("decode response: %v", err)
	}
	wantKeys := []string{
		"case_id", "checker_identity", "file_count", "program_identity",
		"retained_frame_handles", "retained_frame_state", "schema", "semantic",
		"semantic_identity", "shared_product_identity", "tool",
	}
	if got := sortedKeys(response); !equalStrings(got, wantKeys) {
		t.Fatalf("response keys = %#v, want %#v", got, wantKeys)
	}
}

func minimalTestLibraries() *sharedLibraries {
	const directory = "/typokat-wu6-test-libs"
	const library = `
interface Object {}
interface Function {}
interface CallableFunction extends Function {}
interface NewableFunction extends Function {}
interface IArguments {}
interface String {}
interface Number {}
interface Boolean {}
interface RegExp {}
interface Array<T> { length: number; [index: number]: T; }
`
	path := tspath.CombinePaths(directory, "lib.es2025.full.d.ts")
	return &sharedLibraries{
		libDirectory: directory,
		fs:           vfstest.FromMap(map[string]string{path: library}, true),
		files:        make(map[tspath.Path]*ast.SourceFile),
	}
}

func sortedKeys(values map[string]json.RawMessage) []string {
	keys := make([]string, 0, len(values))
	for key := range values {
		keys = append(keys, key)
	}
	sort.Strings(keys)
	return keys
}

func equalStrings(left []string, right []string) bool {
	if len(left) != len(right) {
		return false
	}
	for index := range left {
		if left[index] != right[index] {
			return false
		}
	}
	return true
}
