// tsc 6.0.3 --strict: TS2304 x2, TS2694 x2, and TS2322; exported members survive reopening.
namespace VisibilityNs {
  interface FirstPrivate { first: number }
  export interface FirstPublic { first: number }
  interface PrivateHelper { value: number }
  export interface PublicUsesPrivate { helper: PrivateHelper }
}

namespace VisibilityNs {
  interface SecondPrivate { second: string }
  export interface SecondPublic { second: string }
  let leakedFirst: FirstPrivate; // error[TK2304]: Cannot find name 'FirstPrivate'
  let sharedFirst: FirstPublic;
}

namespace VisibilityNs {
  let leakedSecond: SecondPrivate; // error[TK2304]: Cannot find name 'SecondPrivate'
  let sharedSecond: SecondPublic;
}

let hiddenOutside: VisibilityNs.FirstPrivate; // error[TK2694]: Namespace 'VisibilityNs' has no exported member 'FirstPrivate'
let publicOutside: VisibilityNs.FirstPublic;
declare const publicWithPrivateHelper: VisibilityNs.PublicUsesPrivate;
const retainedPrivateHelperValue: number = publicWithPrivateHelper.helper.value;
const wrongPrivateHelperValue: string = publicWithPrivateHelper.helper.value; // error[TK2322]: Type 'number' is not assignable to type 'string'
let helperStillNotExported: VisibilityNs.PrivateHelper; // error[TK2694]: Namespace 'VisibilityNs' has no exported member 'PrivateHelper'
