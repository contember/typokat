// Backlogs 47 and 51 — two official-suite flow tails remain deliberate under-reports.
// `tsc 6.0.3 --strict --noEmit` reports TS2339 on each `toFixed`/`toString`
// line below, plus TS2454 for the unassigned first value. typokat currently
// reports nothing, so the fixture stays marker-free.

let redundantGuardValue: string | number;

const redundantGuardResult =
  typeof redundantGuardValue === "string" &&
  typeof redundantGuardValue === "string"
    ? redundantGuardValue.substr
    : redundantGuardValue.toFixed;

function assignedOrAlternate(value: number | string | boolean) {
  let captured: number | string | boolean;
  let assigned: number | string | boolean;
  return (
    typeof value === "string" ||
    ((assigned = value) ||
      (typeof value === "number"
        ? (value = 10) && value.toString()
        : (captured = value) && value.toString()))
  );
}
