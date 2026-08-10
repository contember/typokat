// Pending groups remain owned by their exact pattern occurrence. Existing merged storage and
// nested/default syntax cannot turn an unsupported object pattern into a clean exit.
// Oracle: TypeScript 6.0.3 with `--strict --noEmit` reports TS2403 on line 8,
// TS2322 on line 10, TS2345 on line 16, and no diagnostic for the nested-left row.

var b48SharedRestWinner = 1;
const b48SharedRestBefore: number = b48SharedRestWinner;
var { ...b48SharedRestWinner } = { extra: true }; // incomplete[bind/binding-pattern/object-pattern]
const b48SharedRestAfter: number = b48SharedRestWinner;
const b48SharedRestWrong: string = b48SharedRestWinner; // error[TK2322]

declare const b48NestedDefaultSource: { branch?: { nestedLeaf: number } };
const { branch: { nestedLeaf } = { nestedLeaf: 1 } } = b48NestedDefaultSource; // incomplete[bind/binding-pattern/object-pattern]

declare function b48PendingNeedNumber(value: number): number;
const { callback = () => b48PendingNeedNumber("bad"), ...b48DefaultRest } = { // error[TK2345] | incomplete[bind/binding-pattern/object-pattern]
  callback: undefined,
  extra: true,
};
