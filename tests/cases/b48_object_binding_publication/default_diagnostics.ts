// Defaults remove `undefined`, must be compatible with the annotated property, and are checked
// exactly once even when the diagnostic sits inside the default expression.
// Oracle: TypeScript 6.0.3 with `--strict --noEmit`.

declare function b48NeedNumber(value: number): number;

const { directDefault = b48NeedNumber("bad") }: { directDefault?: number } = {}; // error[TK2345]

const {
  callbackDefault = () => b48NeedNumber("nested-bad"), // error[TK2345]
}: { callbackDefault?: () => number } = {};

const { invalidDefault = "bad" }: { invalidDefault?: number } = {}; // error[TK2322]

const { firstLeaf, secondLeaf = firstLeaf }: { firstLeaf: string; secondLeaf?: string } = {
  firstLeaf: "first",
};
const b48FirstLeafWrong: number = firstLeaf; // error[TK2322]
const b48SecondLeafWrong: number = secondLeaf; // error[TK2322]
