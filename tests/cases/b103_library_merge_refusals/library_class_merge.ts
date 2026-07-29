// Backlog 103 correctness tier, incompatible class collision control. A script-top-level
// `class Date` collides with the library's own `Date` in both the type and value spaces. The
// private epoch must process the collision while preserving the library winner downstream.
//
// Oracle: `tsc 6.0.3 --strict --target es2025 --noEmit` reports TS2300 seven times (six inside
// the library sources, once here) plus TS2339 on the member read — the library `Date` wins for
// it too, so typokat's TK2339 agrees. typokat has no TS2300 equivalent; that under-report is
// ledgered in docs/reference/divergences.md under backlog 103.
class Date {
  b103Stamp(): number {
    return 1;
  }
}

// Both witnesses prove the library `Date` survived intact rather than degrading to `any`.
const stamp: Date = new Date();
const wrongStamp: string = new Date().b103Stamp(); // error[TK2339]: Property 'b103Stamp' does not exist
const wrongDate: string = new Date(); // error[TK2322]
