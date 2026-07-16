// tsc 6.0.3 --strict: TS2304 x2, TS2661 x2, TS2749, and TS2694 x4. Typokat
// suppresses the two TS2694 cascades after diagnosing the alias-only local names.
// Resolved exported type-space aliases are clean. Missing local aliases diagnose only
// their declaration sites. Backlog 63 owns the four export-specifier availability records and
// the omitted follow-on diagnostic cardinality.
// An ambient export list switches the block from default-export to explicit-list mode.
declare namespace AmbientAliasList {
  interface HiddenType { hidden: true }
  interface HiddenExplicitType { explicit: true }
  const HiddenValue: 1;
  export {
    HiddenType as PublicType, // incomplete[decl/export-specifier/namespace-payload-unavailable]: namespace export alias target is unavailable
    type HiddenExplicitType as ExplicitTypeOnly, // incomplete[decl/export-specifier/namespace-payload-unavailable]: namespace export alias target is unavailable
    HiddenValue as PublicValue,
    MissingUnused as MissingUnusedAlias, // error[TK2304]: Cannot find name 'MissingUnused' | incomplete[decl/export-specifier/namespace-payload-unavailable]: namespace export alias target is unavailable
    MissingUsed as MissingUsedAlias, // error[TK2304]: Cannot find name 'MissingUsed' | incomplete[decl/export-specifier/namespace-payload-unavailable]: namespace export alias target is unavailable
  };
  interface AfterList { after: true }
}

let publicType: AmbientAliasList.PublicType;
let explicitTypeOnly: AmbientAliasList.ExplicitTypeOnly;
let publicValue: AmbientAliasList.PublicValue; // error[TK2749]: 'AmbientAliasList.PublicValue' refers to a value, but is being used as a type here. Did you mean 'typeof AmbientAliasList.PublicValue'?
let hiddenAfterList: AmbientAliasList.AfterList; // error[TK2694]: Namespace 'AmbientAliasList' has no exported member 'AfterList'
let hiddenOriginal: AmbientAliasList.HiddenType; // error[TK2694]: Namespace 'AmbientAliasList' has no exported member 'HiddenType'
let diagnosedAliasStaysOpaque: AmbientAliasList.MissingUsedAlias;

declare namespace AliasOutputForward {
  interface Local { forward: true }
  export { Local as A };
  export { A as B }; // error[TK2661]: Cannot export 'A'. Only local declarations can be exported from a module
}
let diagnosedForwardAliasUse: AliasOutputForward.B;

declare namespace AliasOutputReverse {
  interface Local { reverse: true }
  export { A as B }; // error[TK2661]: Cannot export 'A'. Only local declarations can be exported from a module
  export { Local as A };
}
let diagnosedReverseAliasUse: AliasOutputReverse.B;

declare namespace GenuineLocalControl {
  interface Local { aliasTarget: true }
  export { Local as A };
  export { A as B };
  interface A { genuineLocal: true }
}
let genuineLocalAliasUse: GenuineLocalControl.B;

declare namespace A {
  namespace N { export interface X {} }
  export { type N as TN };
}
type TypeOnlyNamespaceAlias = A.TN.X;
