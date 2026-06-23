// M6 — reporting mode (§6.4): a failed relation returns a nested reason chain, not a bare
// bool. The inline marker matches the LEAF cause (which is scalar -> stable) inside the
// fully rendered diagnostic; dedicated snapshot tests assert the entire chain.

const nested: { a: { b: number } } = { a: { b: "x" } }; // error[TK2322]: Type 'string' is not assignable to type 'number'
// full rendered chain (snapshot-tested):
//   Type '{ a: { b: string } }' is not assignable to type '{ a: { b: number } }'
//     Types of property 'a' are incompatible
//       Types of property 'b' are incompatible
//         Type 'string' is not assignable to type 'number'

function takesObj(o: { a: number }): number { return o.a; }
const r = takesObj({ a: "x" }); // error[TK2345]: Type 'string' is not assignable to type 'number'
// chain: Argument ... -> Types of property 'a' are incompatible -> 'string' not assignable to 'number'
