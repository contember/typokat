// Missing static properties stay explicit; a default does not make an absent source key legal.
// Oracle: TypeScript 6.0.3 with `--strict --noEmit`.

declare const b48MissingSource: { presentLeaf: number };
const { absentLeaf } = b48MissingSource; // error[TK2339]
const { missingKey: renamedMissing } = b48MissingSource; // error[TK2339]
const { defaultedMissing = 1 } = b48MissingSource; // error[TK2339]

declare const b48NonReadySource: { value: number } | undefined;
const { value: nonReadyLeaf } = b48NonReadySource; // error[TK2339]
