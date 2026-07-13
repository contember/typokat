// Semantic-duplication architecture gate — immutable recursive class applications.
// Cross-checked with tsc 6.0.3 --strict. Every marked line has verdict/code parity.
// The corpus stays disabled until the class-construction graph and publication barrier land.

class RecursiveBox<T> {
  value!: T;
  object!: { node: RecursiveBox<T> };
  array!: RecursiveBox<T>[];
  tuple!: [RecursiveBox<T>, T];
  callback!: (value: RecursiveBox<T>) => RecursiveBox<T>;
  union!: RecursiveBox<T> | null;
  intersection!: RecursiveBox<T> & { stamp: "joined" };
}

declare const stringBox: RecursiveBox<string>;
declare const numberBox: RecursiveBox<number>;

const objectGood: string = stringBox.object.node.value;
const objectBad: number = stringBox.object.node.value; // error[TK2322]: Type 'string' is not assignable to type 'number'
const arrayGood: string = stringBox.array[0].value;
const arrayBad: number = stringBox.array[0].value; // error[TK2322]: Type 'string' is not assignable to type 'number'
const tupleGood: string = stringBox.tuple[0].value;
const tupleBad: number = stringBox.tuple[0].value; // error[TK2322]: Type 'string' is not assignable to type 'number'
const callbackGood: string = stringBox.callback(stringBox).value;
const callbackBad: number = stringBox.callback(stringBox).value; // error[TK2322]: Type 'string' is not assignable to type 'number'
stringBox.callback(numberBox); // error[TK2345]
const unionGood: RecursiveBox<string> | null = stringBox.union;
const unionBad: RecursiveBox<number> | null = stringBox.union; // error[TK2322]
const intersectionGood: string = stringBox.intersection.value;
const intersectionBad: number = stringBox.intersection.value; // error[TK2322]: Type 'string' is not assignable to type 'number'
const intersectionStampGood: "joined" = stringBox.intersection.stamp;
const intersectionStampBad: "other" = stringBox.intersection.stamp; // error[TK2322]: Type '"joined"' is not assignable to type '"other"'

// Class applications remain nested while conditional and mapped nodes are deferred.
class DeferredApplicationSurface<T> {
  conditional!: T extends string
    ? { node: RecursiveBox<T> }
    : { node: RecursiveBox<number> };
  mapped!: { [K in keyof T]: { node: RecursiveBox<T[K]> } };
}

declare const deferredString: DeferredApplicationSurface<string>;
const conditionalGood: string = deferredString.conditional.node.value;
const conditionalBad: number = deferredString.conditional.node.value; // error[TK2322]: Type 'string' is not assignable to type 'number'

declare const deferredBoolean: DeferredApplicationSurface<boolean>;
const conditionalFalseGood: number = deferredBoolean.conditional.node.value;
const conditionalFalseBad: string = deferredBoolean.conditional.node.value; // error[TK2322]: Type 'number' is not assignable to type 'string'

declare const deferredObject: DeferredApplicationSurface<{
  label: string;
  count: number;
}>;
const mappedLabelGood: string = deferredObject.mapped.label.node.value;
const mappedLabelBad: number = deferredObject.mapped.label.node.value; // error[TK2322]: Type 'string' is not assignable to type 'number'
const mappedCountGood: number = deferredObject.mapped.count.node.value;
const mappedCountBad: string = deferredObject.mapped.count.node.value; // error[TK2322]: Type 'number' is not assignable to type 'string'

// Indexed access can select through and from a class application.
class IndexedApplicationSurface<T extends { item: unknown }> {
  node!: RecursiveBox<T["item"]>;

  read(
    box: RecursiveBox<T["item"]>,
  ): RecursiveBox<T["item"]>["value"] {
    return box.value;
  }
}

declare const indexedString: IndexedApplicationSurface<{ item: string }>;
const indexedFieldGood: string = indexedString.node.value;
const indexedFieldBad: number = indexedString.node.value; // error[TK2322]: Type 'string' is not assignable to type 'number'
const indexedMethodGood: string = indexedString.read(stringBox);
const indexedMethodBad: number = indexedString.read(stringBox); // error[TK2322]: Type 'string' is not assignable to type 'number'

// A method binder may constrain and default itself to an enclosing class application.
class MethodApplicationSurface<T> {
  read<U extends RecursiveBox<T> = RecursiveBox<T>>(box: U): U["value"] {
    return box.value;
  }

  defaulted<U extends RecursiveBox<T> = RecursiveBox<T>>(): U {
    throw 0;
  }
}

declare const methodString: MethodApplicationSurface<string>;
const methodDefaultGood: string = methodString.read(stringBox);
const methodDefaultBad: number = methodString.read(stringBox); // error[TK2322]: Type 'string' is not assignable to type 'number'
methodString.read<RecursiveBox<number>>(numberBox); // error[TK2344]
const methodZeroArgDefaultGood: string = methodString.defaulted().value;
const methodZeroArgDefaultBad: number = methodString.defaulted().value; // error[TK2322]: Type 'string' is not assignable to type 'number'

// Mutual applications publish as one SCC regardless of which declaration appears first.
class LeftDeclaredFirst<A, B> {
  right!: RightDeclaredSecond<B, A>;
  yFirst!: A;
  zSecond!: B;
}

class RightDeclaredSecond<A, B> {
  left!: LeftDeclaredFirst<B, A>;
  yFirst!: A;
  zSecond!: B;
}

declare const leftDeclaredFirst: LeftDeclaredFirst<string, number>;
declare const leftDeclaredFirstSwapped: LeftDeclaredFirst<number, string>;
declare const rightDeclaredSecond: RightDeclaredSecond<number, string>;
declare const rightDeclaredSecondSwapped: RightDeclaredSecond<string, number>;
const leftFirstGood: string = leftDeclaredFirst.right.left.yFirst;
const leftFirstBad: number = leftDeclaredFirst.right.left.yFirst; // error[TK2322]: Type 'string' is not assignable to type 'number'
const leftSecondGood: number = leftDeclaredFirst.right.left.zSecond;
const leftSecondBad: string = leftDeclaredFirst.right.left.zSecond; // error[TK2322]: Type 'number' is not assignable to type 'string'
const rightFirstGood: number = leftDeclaredFirst.right.yFirst;
const rightFirstBad: string = leftDeclaredFirst.right.yFirst; // error[TK2322]: Type 'number' is not assignable to type 'string'
const rightSecondGood: string = leftDeclaredFirst.right.zSecond;
const rightSecondBad: number = leftDeclaredFirst.right.zSecond; // error[TK2322]: Type 'string' is not assignable to type 'number'
const leftRelationBadForward: LeftDeclaredFirst<number, string> = leftDeclaredFirst; // error[TK2322]
const leftRelationGoodAfterBad: LeftDeclaredFirst<string, number> = leftDeclaredFirst;
const leftRelationBadForwardAgain: LeftDeclaredFirst<number, string> = leftDeclaredFirst; // error[TK2322]
const leftRelationBadReverse: LeftDeclaredFirst<string, number> = leftDeclaredFirstSwapped; // error[TK2322]
const leftRelationGoodAfterReverse: LeftDeclaredFirst<number, string> = leftDeclaredFirstSwapped;
const rightRelationBadForward: RightDeclaredSecond<string, number> = rightDeclaredSecond; // error[TK2322]
const rightRelationGoodAfterBad: RightDeclaredSecond<number, string> = rightDeclaredSecond;
const rightRelationBadReverse: RightDeclaredSecond<number, string> = rightDeclaredSecondSwapped; // error[TK2322]
const rightRelationGoodAfterReverse: RightDeclaredSecond<string, number> = rightDeclaredSecondSwapped;

class RightDeclaredFirst<T> {
  left!: LeftDeclaredSecond<T>;
}

class LeftDeclaredSecond<T> {
  right!: RightDeclaredFirst<T>;
  zPayload!: T;
}

declare const rightDeclaredFirst: RightDeclaredFirst<string>;
declare const rightDeclaredFirstNumber: RightDeclaredFirst<number>;
declare const leftDeclaredSecond: LeftDeclaredSecond<string>;
declare const leftDeclaredSecondNumber: LeftDeclaredSecond<number>;
const rightOrderGood: string = rightDeclaredFirst.left.right.left.zPayload;
const rightOrderBad: number = rightDeclaredFirst.left.right.left.zPayload; // error[TK2322]: Type 'string' is not assignable to type 'number'
const rightFirstRelationBadForward: RightDeclaredFirst<number> = rightDeclaredFirst; // error[TK2322]
const rightFirstRelationGoodAfterBad: RightDeclaredFirst<string> = rightDeclaredFirst;
const rightFirstRelationBadReverse: RightDeclaredFirst<string> = rightDeclaredFirstNumber; // error[TK2322]
const rightFirstRelationGoodAfterReverse: RightDeclaredFirst<number> = rightDeclaredFirstNumber;
const leftSecondRelationBadForward: LeftDeclaredSecond<number> = leftDeclaredSecond; // error[TK2322]
const leftSecondRelationGoodAfterBad: LeftDeclaredSecond<string> = leftDeclaredSecond;
const leftSecondRelationBadReverse: LeftDeclaredSecond<string> = leftDeclaredSecondNumber; // error[TK2322]
const leftSecondRelationGoodAfterReverse: LeftDeclaredSecond<number> = leftDeclaredSecondNumber;

// Repeated demands must not observe or cache a partially published projection.
class RepeatedDemand<T> {
  next!: RepeatedDemand<T>;
  wrapped!: { next: RepeatedDemand<T> };
  zValue!: T;
}

declare const repeatedDemand: RepeatedDemand<string>;
declare const repeatedDemandNumber: RepeatedDemand<number>;
const repeatedBadFirst: number = repeatedDemand.next.wrapped.next.zValue; // error[TK2322]: Type 'string' is not assignable to type 'number'
const repeatedGood: string = repeatedDemand.next.wrapped.next.zValue;
const repeatedBadSecond: boolean = repeatedDemand.next.wrapped.next.zValue; // error[TK2322]: Type 'string' is not assignable to type 'boolean'
const repeatedGoodAgain: string = repeatedDemand.next.wrapped.next.zValue;
const repeatedRelationBadForward: RepeatedDemand<number> = repeatedDemand; // error[TK2322]
const repeatedRelationGoodAfterBad: RepeatedDemand<string> = repeatedDemand;
const repeatedRelationBadForwardAgain: RepeatedDemand<number> = repeatedDemand; // error[TK2322]
const repeatedRelationGoodAfterRepeat: RepeatedDemand<string> = repeatedDemand;
const repeatedRelationBadReverse: RepeatedDemand<string> = repeatedDemandNumber; // error[TK2322]
const repeatedRelationGoodAfterReverse: RepeatedDemand<number> = repeatedDemandNumber;

// Non-regular recursion grows the argument one layer per demand and must still terminate.
class ExpandingBox<T> {
  next!: ExpandingBox<T[]>;
  value!: T;
}

declare const expandingBox: ExpandingBox<string>;
declare const expandingBoxNumber: ExpandingBox<number>;
const expandingLevelZero: string = expandingBox.value;
const expandingLevelOne: string[] = expandingBox.next.value;
const expandingLevelTwo: string[][] = expandingBox.next.next.value;
const expandingBad: string[] = expandingBox.next.next.value; // error[TK2322]: Type 'string[][]' is not assignable to type 'string[]'
const expandingRelationBadForward: ExpandingBox<number> = expandingBox; // error[TK2322]
const expandingRelationGoodAfterBad: ExpandingBox<string> = expandingBox;
const expandingRelationBadForwardAgain: ExpandingBox<number> = expandingBox; // error[TK2322]
const expandingRelationGoodAfterRepeat: ExpandingBox<string> = expandingBox;
const expandingRelationBadReverse: ExpandingBox<string> = expandingBoxNumber; // error[TK2322]
const expandingRelationGoodAfterReverse: ExpandingBox<number> = expandingBoxNumber;

// Projection preserves private/protected declaring-class identity instead of flattening it away.
class PrivateApplication<T> {
  next!: PrivateApplication<T>;
  private token!: T;
  value!: T;
}

class ForeignPrivateApplication<T> {
  next!: ForeignPrivateApplication<T>;
  private token!: T;
  value!: T;
}

declare const privateString: PrivateApplication<string>;
declare const privateNumber: PrivateApplication<number>;
declare const foreignPrivateString: ForeignPrivateApplication<string>;
const privateGood: PrivateApplication<string> = privateString;
const privateArgumentBadForward: PrivateApplication<number> = privateString; // error[TK2322]
const privateArgumentBadReverse: PrivateApplication<string> = privateNumber; // error[TK2322]
const privateOriginBadForward: PrivateApplication<string> = foreignPrivateString; // error[TK2322]
const privateOriginBadReverse: ForeignPrivateApplication<string> = privateString; // error[TK2322]

class PrivateLeafApplication<T> {
  next!: PrivateLeafApplication<T>;
  private token!: T;
  value!: T;
}

class ForeignPrivateLeafApplication<T> {
  next!: ForeignPrivateLeafApplication<T>;
  private token!: T;
  value!: T;
}

class PrivateCarrierApplication<T> {
  child!: PrivateLeafApplication<T>;
}

class ForeignPrivateCarrierApplication<T> {
  child!: ForeignPrivateLeafApplication<T>;
}

declare const privateCarrier: PrivateCarrierApplication<string>;
declare const foreignPrivateCarrier: ForeignPrivateCarrierApplication<string>;
const nestedPrivateOriginBadForward: PrivateCarrierApplication<string> = foreignPrivateCarrier; // error[TK2322]
const nestedPrivateOriginBadReverse: ForeignPrivateCarrierApplication<string> = privateCarrier; // error[TK2322]

class ProtectedApplication<T> {
  next!: ProtectedApplication<T>;
  protected token!: T;
  value!: T;
}

class ForeignProtectedApplication<T> {
  next!: ForeignProtectedApplication<T>;
  protected token!: T;
  value!: T;
}

declare const protectedString: ProtectedApplication<string>;
declare const foreignProtectedString: ForeignProtectedApplication<string>;
const protectedGood: ProtectedApplication<string> = protectedString;
const protectedOriginBadForward: ProtectedApplication<string> = foreignProtectedString; // error[TK2322]
const protectedOriginBadReverse: ForeignProtectedApplication<string> = protectedString; // error[TK2322]

// Constructor parameter properties retain their application types and readonly behavior.
class ParameterPropertyApplication<T> {
  constructor(
    public current: RecursiveBox<T>,
    readonly history: RecursiveBox<T>[],
  ) {}

  update(next: RecursiveBox<T>): void {
    this.current = next;
    this.history = [next]; // error[TK2540]: Cannot assign to 'history' because it is a read-only property
  }
}

declare const parameterProperty: ParameterPropertyApplication<string>;
const parameterPropertyGood: string = parameterProperty.current.value;
const parameterPropertyBad: number = parameterProperty.current.value; // error[TK2322]: Type 'string' is not assignable to type 'number'
const parameterHistoryGood: string = parameterProperty.history[0].value;
const parameterHistoryBad: number = parameterProperty.history[0].value; // error[TK2322]: Type 'string' is not assignable to type 'number'
new ParameterPropertyApplication<string>(numberBox, []); // error[TK2345]

// Surface lowering owns this unresolved parameter-property annotation exactly once.
class MissingParameterPropertyApplication {
  constructor(
    public missing: RecursiveBox<MissingConstructionType>, // error[TK2304]: Cannot find name 'MissingConstructionType'
  ) {}
}

// Constructor overloads retain source order and hide the implementation signature.
class OverloadedApplicationConstructor<T> {
  constructor(value: RecursiveBox<T>);
  constructor(value: RecursiveBox<T>[]);
  constructor(public value: RecursiveBox<T> | RecursiveBox<T>[] | boolean) {}
}

new OverloadedApplicationConstructor<string>(true); // error[TK2769]: No overload matches this call

// Static class-parameter diagnostics are emitted once per source occurrence: three
// signature/field events and one later body event, with no span-based suppression.
class StaticApplicationCardinality<T> {
  static cached: RecursiveBox<T>; // error[TK2302]: Static members cannot reference class type parameters

  static inspect(
    value: RecursiveBox<T>, // error[TK2302]: Static members cannot reference class type parameters
  ): RecursiveBox<T> { // error[TK2302]: Static members cannot reference class type parameters
    const local: RecursiveBox<T> = value; // error[TK2302]: Static members cannot reference class type parameters
    return local;
  }
}
