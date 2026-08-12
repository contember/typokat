# Backlog 42 enum acceptance corpus

This disabled WU0 corpus is the acceptance spec for the enum type/value model. Every file is
cross-checked independently with TypeScript 6.0.3:

```sh
tsc --strict --noEmit --pretty false --skipLibCheck <fixture>
```

The implementation may enable this directory only when every family below has a witness:

- [x] numeric, string, heterogeneous, ambient, and `const enum` declarations;
- [x] whole-enum/member/raw-literal/cross-enum and equal-value alias assignability;
- [x] identifier, quoted, computed-string, and computed-template member names;
- [x] implicit, constant, prior-member, negative, and opaque computed initializers;
- [x] invalid auto-increment, string-valued computed, and const-enum initializer diagnostics;
- [x] generic, array, object-property, union, and intersection transport;
- [x] equality and `switch` member narrowing, including the default complement;
- [x] repeated enum and enum+namespace merges in both declaration orders;
- [x] enum+function and three-way enum/function/namespace diagnostics and typed recovery.

Project publication has a separate disabled sibling,
[`../b42_enum_project_model/`](../b42_enum_project_model/), because the conformance harness treats
flat and project-shaped corpora as distinct directories.
