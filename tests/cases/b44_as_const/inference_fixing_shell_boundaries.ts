declare const fixingFlag: boolean;
declare function fixingShell<T>(value: T, callback: (value: T) => void): T;
declare function fixingObjectShell<T>(value: T, callback: { run(value: T): void }): T;
declare function fixingTwoBinders<T, U>(first: T, second: U, callback: (value: U) => void): T;

const fixingArrow = fixingShell("read", value => {});
const exactFixingArrow: "read" = fixingArrow; // error[TK2322]
const fixingFunction = fixingShell("read", function(value) {});
const exactFixingFunction: "read" = fixingFunction; // error[TK2322]
const fixingMethod = fixingObjectShell("read", { run(value) {} });
const exactFixingMethod: "read" = fixingMethod; // error[TK2322]
const fixingConditional = fixingShell("read", fixingFlag ? value => {} : value => {});
const exactFixingConditional: "read" = fixingConditional; // error[TK2322]
const fixingMixedConditional = fixingShell("read", fixingFlag ? value => {} : (value: string) => {});
const exactFixingMixedConditional: "read" = fixingMixedConditional;
const fixingOmittedConditional = fixingShell("read", fixingFlag ? value => {} : () => {});
const exactFixingOmittedConditional: "read" = fixingOmittedConditional; // error[TK2322]
declare const maybeFixingCallback: ((value: string) => void) | undefined;
const fixingCallableLeft = fixingShell("read", maybeFixingCallback ?? (value => {}));
const exactFixingCallableLeft: "read" = fixingCallableLeft;
declare const fixingUndefined: undefined;
const fixingUndefinedLeft = fixingShell("read", fixingUndefined || (value => {}));
const exactFixingUndefinedLeft: "read" = fixingUndefinedLeft; // error[TK2322]
const fixingOmitted = fixingShell("read", function() {});
const exactFixingOmitted: "read" = fixingOmitted;
const fixingOtherBinder = fixingTwoBinders("read", "write", value => {});
const exactFixingOtherBinder: "read" = fixingOtherBinder;

const fixingRegular = fixingShell("read" as const, fixingFlag ? value => {} : value => {});
const exactFixingRegular: "read" = fixingRegular;
let fixingRegularCopy = fixingRegular;
fixingRegularCopy = "write"; // error[TK2322]
