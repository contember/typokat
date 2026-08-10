// Excluded binding routes keep their boundary while walking each nested default exactly once.
// Oracle: TypeScript 6.0.3 with `--strict --noEmit` reports TS2345 on lines 9, 15,
// and 22. The strict unknown catch source also reports TS2339 and TS2700 on line 22;
// typokat's existing catch-parameter boundary owns that deferred surface.

declare function b48RouteNeedNumber(value: number): number;

function b48ExcludedParameter(
  { callback = () => b48RouteNeedNumber("parameter"), ...rest }: { // error[TK2345] | incomplete[bind/binding-pattern/object-pattern]
    callback?: () => number;
    extra: boolean;
  },
): void {}

for (const { callback = () => b48RouteNeedNumber("for-of"), ...rest } of [ // error[TK2345] | incomplete[bind/binding-pattern/object-pattern]
  { callback: undefined, extra: true },
]) {
}

try {
  throw { callback: undefined, extra: true };
} catch ({ callback = () => b48RouteNeedNumber("catch"), ...rest }) { // error[TK2345] | incomplete[stmt-check/try-statement/catch-param]
}
