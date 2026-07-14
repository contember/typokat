// Project semantic-duplication gate — checked before its importer but ordered after it by input.
// The resolved initializer is valid in tsc 6.0.3. typokat owns its initializer-inference record here;
// the local heritage cycle owns separate cycle records and is not replayed into the importer.

const orderedSeed = 1;

export class OrderedPoisonedBase {
  value = orderedSeed; // incomplete[class/property-definition/initializer-inference]: unannotated field initializer cannot be inferred during class surface construction
}

class OrderedCycleLeft extends OrderedCycleRight {} // incomplete[class/class-heritage/cycle]: class heritage cycle poisons the published surface
class OrderedCycleRight extends OrderedCycleLeft {} // incomplete[class/class-heritage/cycle]: class heritage cycle poisons the published surface

const baseDiagnostic: string = 1; // error[TK2322]: Type 'number' is not assignable to type 'string'
