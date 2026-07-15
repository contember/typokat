// tsc 6.0.3 --strict: TS2304 x2, TS2749, and TS2694 x2; both resolved exported
// type-space aliases are clean. Missing local aliases diagnose only their declaration sites.
// An ambient export list switches the block from default-export to explicit-list mode.
declare namespace AmbientAliasList {
  interface HiddenType { hidden: true }
  interface HiddenExplicitType { explicit: true }
  const HiddenValue: 1;
  export {
    HiddenType as PublicType,
    type HiddenExplicitType as ExplicitTypeOnly,
    HiddenValue as PublicValue,
    MissingUnused as MissingUnusedAlias, // error[TK2304]: Cannot find name 'MissingUnused'
    MissingUsed as MissingUsedAlias, // error[TK2304]: Cannot find name 'MissingUsed'
  };
  interface AfterList { after: true }
}

let publicType: AmbientAliasList.PublicType;
let explicitTypeOnly: AmbientAliasList.ExplicitTypeOnly;
let publicValue: AmbientAliasList.PublicValue; // error[TK2749]: 'AmbientAliasList.PublicValue' refers to a value, but is being used as a type here. Did you mean 'typeof AmbientAliasList.PublicValue'?
let hiddenAfterList: AmbientAliasList.AfterList; // error[TK2694]: Namespace 'AmbientAliasList' has no exported member 'AfterList'
let hiddenOriginal: AmbientAliasList.HiddenType; // error[TK2694]: Namespace 'AmbientAliasList' has no exported member 'HiddenType'
let diagnosedAliasStaysOpaque: AmbientAliasList.MissingUsedAlias;
