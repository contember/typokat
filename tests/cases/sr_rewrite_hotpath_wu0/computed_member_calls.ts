// tsc 6.0.3 --strict: only the three BadComputedReceiver calls report TS2684.

interface GoodComputedReceiver {
    n: number;
    method(this: { n: number }): void;
}

declare const good: GoodComputedReceiver;
good["method"]();
(good["method"])();
((good)["method"])();

interface BadComputedReceiver {
    method(this: { n: number }): void;
}

declare const bad: BadComputedReceiver;
bad["method"](); // error[TK2684]
(bad["method"])(); // error[TK2684]
((bad)["method"])(); // error[TK2684]

interface GenericComputedReceiver {
    tag: "holder";
    method<T>(this: T): T;
}

declare const genericHolder: GenericComputedReceiver;
const computedGeneric: GenericComputedReceiver = genericHolder["method"]();
const parenthesizedComputedGeneric: GenericComputedReceiver = (genericHolder["method"])();
const staticGeneric: GenericComputedReceiver = genericHolder.method();
