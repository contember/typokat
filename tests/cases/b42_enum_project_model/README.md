# Backlog 42 enum project publication corpus

Each subdirectory is one strict files-only ESNext/Bundler project. The raw/internal conformance
route keeps this config-backed corpus disabled. WU4 runs every project through the black-box CLI
contract after enum value/type slots publish through named imports, type-only imports, acyclic
source re-exports, namespace payloads, and the admitted identifier-default form. Every project has
normal and reverse `files` order configs; both orders must produce byte-identical output.

The project checklist is explicit so review can compare planned and enabled coverage:

- [x] direct named export/import publishes paired value and type slots;
- [x] a type-only import retains the type slot and erases the value slot;
- [x] the existing `b15_acyclic_source_reexports/05b_enum_value_reexport` row preserves enum
  identity across the barrel;
- [x] an enum+namespace export retains enum members plus namespace value/type payloads;
- [x] an identifier default exports the paired enum slots only for a bare enum;
- [x] every route has normal and reverse project-root order configs.

TypeScript reports `TS1361` for the erased value use. Typokat retains its documented `TK2304`
stand-in for this type-only import boundary.
