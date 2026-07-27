// Backlog 103 control, the backlog-102 regression net across files. Fresh script globals must
// still publish into the delta-side global scope and refuse nothing.
//
// Oracle: `tsc 6.0.3 --strict --target es2025 --noEmit` reports TS2322 once, in 99_consume.ts.
interface B103CrossShape {
  label: string;
}

declare var b103CrossValue: B103CrossShape;
