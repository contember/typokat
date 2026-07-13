// tsc 6.0.3 --strict: only the final number assignment reports TS2322.
// Ordered aliases create a mapped-value object spine without parser nesting.

type Layer00<T> = { value: T };
type Layer01<T> = { next: Layer00<T> };
type Layer02<T> = { next: Layer01<T> };
type Layer03<T> = { next: Layer02<T> };
type Layer04<T> = { next: Layer03<T> };
type Layer05<T> = { next: Layer04<T> };
type Layer06<T> = { next: Layer05<T> };
type Layer07<T> = { next: Layer06<T> };
type Layer08<T> = { next: Layer07<T> };
type Layer09<T> = { next: Layer08<T> };
type Layer10<T> = { next: Layer09<T> };
type Layer11<T> = { next: Layer10<T> };
type Layer12<T> = { next: Layer11<T> };
type Layer13<T> = { next: Layer12<T> };
type Layer14<T> = { next: Layer13<T> };
type Layer15<T> = { next: Layer14<T> };
type Layer16<T> = { next: Layer15<T> };

type DeepMap<T> = { [K in keyof T]: Layer16<T[K]> };
type Result = DeepMap<{ a: "leaf" }>;

declare const result: Result;
const preserved: "leaf" = result.a.next.next.next.next.next.next.next.next.next.next.next.next.next.next.next.next.value;
const rejected: number = result.a.next.next.next.next.next.next.next.next.next.next.next.next.next.next.next.next.value; // error[TK2322]
