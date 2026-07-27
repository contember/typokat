// Backlog 103, the guard tier. A script-top-level `class Date` collides with the library's own
// `Date` in both the type and the value space. The type half reached the frozen group and
// panicked; it is now a recorded refusal, and the library `Date` still wins everywhere.
//
// Oracle: `tsc 6.0.3 --strict --target es2025 --noEmit` reports TS2300 seven times (six inside
// the library sources, once here) plus TS2339 on the member read — the library `Date` wins for
// it too, so typokat's TK2339 agrees. typokat has no TS2300 equivalent; that under-report is
// ledgered in docs/reference/divergences.md under backlog 103.
class Date { // incomplete[bind/frozen-library-global/merge-refused]: user declaration cannot merge into the frozen default-library global
  b103Stamp(): number {
    return 1;
  }
}

// Both witnesses prove the library `Date` survived intact rather than degrading to `any`.
const stamp: Date = new Date();
const wrongStamp: string = new Date().b103Stamp(); // error[TK2339]: Property 'b103Stamp' does not exist
const wrongDate: string = new Date(); // error[TK2322]
