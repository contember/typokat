// WU0 addendum — class construction and private origin survive interface/namespace augmentation.
class NominalClassMerge {
  static existing = 1;
  private identity = 1;
  instance = 1;
}
interface NominalClassMerge {
  augmented: string;
  recursive: NominalClassMerge;
}
namespace NominalClassMerge {
  export const added = "nominal";
  export interface Nested { nested: boolean }
}

class ForeignNominalClass {
  private identity = 1;
  instance = 1;
  augmented = "foreign";
  recursive = this;
}

const nominalConstructed: NominalClassMerge = new NominalClassMerge();
const nominalAugmentedWrong: number = nominalConstructed.augmented; // error[TK2322]: Type 'string' is not assignable to type 'number'
const nominalRecursiveWrong: number = nominalConstructed.recursive.augmented; // error[TK2322]: Type 'string' is not assignable to type 'number'
const nominalExistingWrong: string = NominalClassMerge.existing; // error[TK2322]: Type 'number' is not assignable to type 'string'
const nominalAddedWrong: number = NominalClassMerge.added; // error[TK2322]: Type 'string' is not assignable to type 'number'
const nominalNested: NominalClassMerge.Nested = { nested: true };
const nominalNestedWrong: number = nominalNested.nested; // error[TK2322]: Type 'boolean' is not assignable to type 'number'
const nominalOriginWrong: NominalClassMerge = new ForeignNominalClass(); // error[TK2322]
