// Lexical leaves shadow independently. `var` leaves belong to the containing function and are
// visible before execution without leaking out of that function.
// Oracle: TypeScript 6.0.3 with `--strict --noEmit`.

let b48Shadowed = "outer";
{
  const { b48Shadowed } = { b48Shadowed: 1 };
  const b48InnerShadow: number = b48Shadowed;
  const b48InnerShadowWrong: string = b48Shadowed; // error[TK2322]
}
const b48OuterShadow: string = b48Shadowed;
const b48OuterShadowWrong: number = b48Shadowed; // error[TK2322]

function b48VarOwner(): void {
  const b48BeforeVar = ownedLeaf; // error[TK2454]
  if (true) {
    var { ownedLeaf } = { ownedLeaf: "owned" };
  }
  const b48OwnedLeaf: string = ownedLeaf;
  const b48OwnedLeafWrong: number = ownedLeaf; // error[TK2322]
}

ownedLeaf; // error[TK2304]: Cannot find name 'ownedLeaf'
