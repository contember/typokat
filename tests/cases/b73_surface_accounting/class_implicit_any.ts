// A class property with neither an annotation nor an initializer is omitted from the published
// class surface today. Keep that implicit-any member explicit until backlog 48 emits TK7008.

interface CastSource {
  source: string;
}

class UntypedTarget {
  missing; // incomplete[class/property-definition/implicit-any]: class property without an annotation or initializer has implicit any type
}

class ExplicitAnyTarget {
  missing: any;
}

declare const source: CastSource;

// The missing member must not let this target appear as an empty, permissive class surface.
const hiddenCast = source as UntypedTarget;

// Control: an explicit `any` member is a complete surface and remains usable.
const explicit = new ExplicitAnyTarget();
explicit.missing = 1;
