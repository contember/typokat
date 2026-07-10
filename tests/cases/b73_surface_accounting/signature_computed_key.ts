// Surface-accounting spec (backlog 75). ENABLED by WU5: object-type and interface member
// collection record the incomplete surface for a computed property- or method-signature
// key before dropping the member. See tests/cases/README.md ("Surface-accounting corpus").
//
// Skip accounted: `lower_object_annotation` / `lower_interface_members` skipped a member
// whose `static_name()` is `None` (a computed key) silently — for BOTH property and
// method signatures (the method arm was the WU7-E F1 finding). WU5 records it first.
// tsc 6.0.3 --strict: TS2304/TS1169-family on an unresolved/invalid computed key.

function foo(): number {
  return 1;
}

// INCOMPLETE: a computed key in a type-literal alias (interface member path).
type Lit = { [foo()]: number }; // incomplete[signature/property-signature/computed-key]

// INCOMPLETE: a computed key in an interface.
interface I { [foo()]: number; } // incomplete[signature/property-signature/computed-key]

// INCOMPLETE: a computed key in an inline object type (object-annotation path).
let o: { [foo()]: number } = {} as never; // incomplete[signature/property-signature/computed-key]

// INCOMPLETE (F1): a computed METHOD-signature key in a type-literal alias.
type MLit = { [foo()](): void }; // incomplete[signature/method-signature/computed-key]

// INCOMPLETE (F1): a computed METHOD-signature key in an interface.
interface MI { [foo()](): void; } // incomplete[signature/method-signature/computed-key]

// INCOMPLETE (F1): a computed method-signature key in an inline object type.
let m: { [foo()](): void } = {} as never; // incomplete[signature/method-signature/computed-key]
