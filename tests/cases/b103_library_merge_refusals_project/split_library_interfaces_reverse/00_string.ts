// Backlog 103, the guard tier — the same split merge with the files fed in the opposite order
// (this consumer sorts first, so it is bound before the file it reads). Input order must not
// change any per-source outcome: both declarations are still refused and recorded at their own
// site, and both reads still over-report exactly as in split_library_interfaces/.
//
// Oracle: identical to split_library_interfaces/.
interface String { // incomplete[bind/frozen-library-global/merge-refused]: user declaration cannot merge into the frozen default-library global
  b103ReverseUpper(): string;
}

declare const view: Window;
const reverseFlag: boolean = view.b103ReverseFlag; // error[TK2339]: Property 'b103ReverseFlag' does not exist
const reverseUpper: string = "quiet".b103ReverseUpper(); // error[TK2339]: Property 'b103ReverseUpper' does not exist
