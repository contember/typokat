// WU6A adversarial oracle: tsc 6.0.3 --strict --noEmit --pretty false --lib es5 --module commonjs.
// The ambient export alias is an exact support target: its target type/storage must become the
// public property, while the original private spelling remains absent.

declare namespace Wu6aReviewAmbientAlias {
  const hidden: number;
  export { hidden as value };
}

const wu6aReviewAmbientAliasRoot = Wu6aReviewAmbientAlias;
const wu6aReviewAmbientAliasValue: number = Wu6aReviewAmbientAlias.value;
const wu6aReviewAmbientAliasWrong: string = Wu6aReviewAmbientAlias.value; // error[TK2322]: Type 'number' is not assignable to type 'string'
Wu6aReviewAmbientAlias.hidden; // error[TK2339]: Property 'hidden' does not exist
