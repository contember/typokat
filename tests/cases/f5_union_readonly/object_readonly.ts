// F5 / backlog 03 (part a) — `readonly` is enforced on OBJECT-TYPE and INTERFACE
// members, not just class fields. Object-type-literal and interface lowering
// previously dropped the `readonly` modifier, so assigning to such a member was
// silently allowed (false negative). Cross-checked against tsc 6.0.3.

interface I { readonly k: number; }
declare const i: I;
i.k = 5;                          // error[TK2540]: Cannot assign to 'k' because it is a read-only property

declare const o: { readonly k: number };
o.k = 5;                          // error[TK2540]: Cannot assign to 'k' because it is a read-only property

declare const m: { k: number };
m.k = 5;                          // ok — mutable
m.k = "x";                        // error[TK2322]: not assignable to type 'number'
