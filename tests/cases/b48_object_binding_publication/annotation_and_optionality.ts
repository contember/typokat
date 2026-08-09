// An aggregate pattern annotation is the source type. It wins over a narrower initializer.
// Oracle: TypeScript 6.0.3 with `--strict --noEmit`.

const { widenedLeaf }: { widenedLeaf: number } = { widenedLeaf: 1 };
const b48WidenedLeaf: number = widenedLeaf;
const b48LiteralMustNotSurvive: 1 = widenedLeaf; // error[TK2322]

const b48OptionalSource: { maybe?: number; label?: string } = {};
const { maybe, label = "fallback" } = b48OptionalSource;
const b48Maybe: number | undefined = maybe;
const b48MaybeWrong: number = maybe; // error[TK2322]
const b48Label: string = label;
const b48LabelWrong: undefined = label; // error[TK2322]

const { renamed: renamedOptional = 1 }: { renamed?: number } = {};
const b48RenamedOptional: number = renamedOptional;
const b48RenamedOptionalWrong: string = renamedOptional; // error[TK2322]
