# Semantic-duplication acceptance fixtures

This corpus is intentionally disabled in `tests/conformance.rs` until the class-application rollout
is complete. The existing marker-parser tests still scan every file, so malformed disabled markers
fail immediately.

Fixture inventory:

- `class_member_surfaces.ts` retains callable binders once and preserves diagnostic cardinality,
  overload hiding, parameter properties, and the documented omitted-return behavior.
- `class_initializer_supported.ts` and `class_initializer_unsupported.ts` cover representative
  supported and unsupported initializer boundaries. Every unsupported unannotated field owns exactly one
  `class/property-definition/initializer-inference` record.
- `class_initializer_poison.ts` pins whole-class poison origins and heritage propagation. Later
  read/write/relation/new/static demands own no new public record.
- `class_application_contract.ts` closes class argument arity, defaults, unavailable explicit
  arguments, constructor inference, and self/open-frame application behavior.
- `class_scheduling_shapes.ts` covers finite class/alias/interface identity cycles.
- `class_projection_exhaustion.ts` covers externally observable projection-budget boundaries and
  candidate/inference controls.
- `recursive_class_applications.ts` covers projection through represented composite types,
  declaration order, repeated demand, nominal origin, and construction surfaces.
- `selector_precedence.ts` preserves the ordinary non-exhausted overload-selection baseline.

The marker harness cannot establish the following mandatory direct Rust gates:

- an exhaustive match/classifier with an explicit arm for every current oxc `Expression` variant,
  where only explicitly listed pure shapes return `Inferred`, every other explicit arm returns
  `Unsupported`, and compilation or a coverage test forces every new AST variant to be classified;
- exact event order/spans, graph-edge completeness, SCC atomicity, and the exact 128/129 admission
  boundary;
- poison before same-pair identity, relation-cache lookup, `new` projection, and call/overload/inference
  selection;
- incomplete application vectors never entering type interning;
- invalid arity remaining the primary application cause while every nested child record still emits;
- `Unsupported` default declaration-event to application-event linkage;
- candidate exhaustion before a later winner remaining `Exhausted`;
- no fabricated recovery operand enters a `ClassInstance`, projection, or relation; no recovery path
  performs durable memo/cache writes; and exactly the preallocated owner record exists, with zero
  replay/additional event writes;
- typed exhaustion variants, frozen type-parameter metadata, and one-time lowering counters.

These source fixtures pin only public diagnostic/incomplete cardinality and downstream behavior.
