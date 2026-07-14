// Semantic-duplication architecture gate — whole-class initializer poison.
// tsc 6.0.3 --strict reports ordinary readonly/private/protected/structural/constructor/call-arity/cyclic-base
// diagnostics on selected demand controls. typokat instead exposes only the five initializer
// origins, owned heritage events, and independent method-body TK2322; demands fabricate no types.

const poisonSeed = 1;

class PoisonedInitializerSurface {
  mutableOrigin = poisonSeed; // incomplete[class/property-definition/initializer-inference]: unannotated field initializer cannot be inferred during class surface construction
  readonly readonlyOrigin = poisonSeed; // incomplete[class/property-definition/initializer-inference]: unannotated field initializer cannot be inferred during class surface construction
  private privateOrigin = poisonSeed; // incomplete[class/property-definition/initializer-inference]: unannotated field initializer cannot be inferred during class surface construction
  protected protectedOrigin = poisonSeed; // incomplete[class/property-definition/initializer-inference]: unannotated field initializer cannot be inferred during class surface construction
  static staticOrigin = poisonSeed; // incomplete[class/property-definition/initializer-inference]: unannotated field initializer cannot be inferred during class surface construction
  safe!: number;

  bodyStillChecks(): void {
    const bodyMismatch: string = 1; // error[TK2322]: Type 'number' is not assignable to type 'string'
  }
}

declare const poisonedInitializer: PoisonedInitializerSurface;

// Every demand below receives typed initializer-poison exhaustion internally. None owns a new record.
const poisonedRead = poisonedInitializer.mutableOrigin;
poisonedInitializer.mutableOrigin = 2;
poisonedInitializer.readonlyOrigin = 2;
const poisonedPrivateRead = poisonedInitializer.privateOrigin;
const poisonedProtectedRead = poisonedInitializer.protectedOrigin;
const poisonedStaticRead = PoisonedInitializerSurface.staticOrigin;
PoisonedInitializerSurface.staticOrigin = 2;

const poisonedAsStructuralSource: { safe: number } = poisonedInitializer;
declare const structuralSource: { safe: number };
const poisonedAsStructuralTarget: PoisonedInitializerSurface = structuralSource;
const poisonedSamePair: PoisonedInitializerSurface = poisonedInitializer;
const poisonedNew = new PoisonedInitializerSurface("unexpected");
poisonedInitializer.bodyStillChecks("unexpected");

// Ordinary identity references publish normally and do not propagate poison.
type PoisonedInitializerAlias = PoisonedInitializerSurface;
class OrdinaryPoisonReference {
  property!: PoisonedInitializerSurface;
  method(value: PoisonedInitializerSurface): PoisonedInitializerSurface {
    return value;
  }
}
declare const ordinaryPoisonReference: OrdinaryPoisonReference;
const laterPoisonDemand = ordinaryPoisonReference.property.mutableOrigin;

class PoisonedDerivedOne extends PoisonedInitializerSurface {} // incomplete[class/class-heritage/poisoned-base]
class PoisonedDerivedTwo extends PoisonedDerivedOne {} // incomplete[class/class-heritage/poisoned-base]

class HeritageCycleLeft extends HeritageCycleRight {} // incomplete[class/class-heritage/cycle]
class HeritageCycleRight extends HeritageCycleLeft {} // incomplete[class/class-heritage/cycle]
