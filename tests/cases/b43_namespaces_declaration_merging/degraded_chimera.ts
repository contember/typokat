// tsc 6.0.3 --strict: TS2567 on enum/function, TS2434 on namespace, plus the three
// receiver-use errors below.
enum DegradedChimera { A } // error[TK2567]
namespace DegradedChimera { // error[TK2434]: A namespace declaration cannot be located prior to a class or function with which it is merged
  export const tag = "tag";
}
function DegradedChimera(): void {} // error[TK2567]
const chimeraTagWrong: number = DegradedChimera.tag; // error[TK2322]: Type 'string' is not assignable to type 'number'
const chimeraCallWrong: number = DegradedChimera(); // error[TK2322]: Type 'void' is not assignable to type 'number'
const chimeraMissing: string = DegradedChimera.missing; // error[TK2339]: Property 'missing' does not exist on type 'typeof DegradedChimera'
