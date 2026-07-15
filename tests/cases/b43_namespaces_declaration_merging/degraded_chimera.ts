// tsc 6.0.3 --strict: TS2567 on enum/function, TS2434 on namespace, plus the three
// receiver-use errors below. Exact TS2567 ownership is deferred to the WU0A/direct-test gate.
// typokat deliberately keeps enum semantics out of backlog 43, so the enum remains an
// explicit incomplete surface and cannot silently turn this three-way chimera permissive.
enum DegradedChimera { A } // incomplete[decl/enum-declaration/self]
namespace DegradedChimera { // error[TK2434]: A namespace declaration cannot be located prior to a class or function with which it is merged
  export const tag = "tag";
}
function DegradedChimera(): void {}
const chimeraTagWrong: number = DegradedChimera.tag; // error[TK2322]: Type 'string' is not assignable to type 'number'
const chimeraCallWrong: number = DegradedChimera(); // error[TK2322]: Type 'void' is not assignable to type 'number'
const chimeraMissing: string = DegradedChimera.missing; // error[TK2339]: Property 'missing' does not exist on type 'typeof DegradedChimera'
