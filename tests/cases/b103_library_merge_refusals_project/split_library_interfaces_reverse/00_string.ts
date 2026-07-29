// Backlog 103 correctness tier — the same split merge with files fed in the opposite order (this
// consumer sorts first, so it is bound before the file it reads). Input order must not change the
// merged result.
//
// Oracle: identical to split_library_interfaces/.
interface String {
  b103ReverseUpper(): string;
}

declare const view: Window;
const reverseFlag: boolean = view.b103ReverseFlag;
const reverseUpper: string = "quiet".b103ReverseUpper();
