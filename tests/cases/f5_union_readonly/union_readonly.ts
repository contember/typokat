// F5 / backlog 03 (part b) — member-assignment through a UNION base. Previously a
// union base fell through the object-only property lookup and was silently skipped
// (no readonly check, no type check). Now the property is resolved across the union
// members and enforced: `readonly` if readonly on ANY member (matching tsc), and the
// assigned value must be assignable to the member type. Cross-checked against tsc 6.0.3.

type A = { readonly value: number; tagA: 1 };
type B = { readonly value: number; tagB: 2 };
declare const u: A | B;
u.value = 12;                     // error[TK2540]: Cannot assign to 'value' because it is a read-only property

type E = { value: number; tagE: 1 };
type F = { value: number; tagF: 2 };
declare const v: E | F;
v.value = 12;                     // ok — mutable on both members
v.value = "x";                    // error[TK2322]: not assignable to type 'number'

type C = { value: number; tagC: 1 };           // mutable
type D = { readonly value: number; tagD: 2 };  // readonly
declare const w: C | D;
w.value = 12;                     // error[TK2540]: Cannot assign to 'value' because it is a read-only property

// Item 03's literal repro: structurally identical members canonicalize to one
// object (not a real union), so this also exercises the part-(a) object-readonly fix.
type P = { readonly value: number };
type Q = { readonly value: number };
declare const pq: P | Q;
pq.value = 12;                    // error[TK2540]: Cannot assign to 'value' because it is a read-only property
