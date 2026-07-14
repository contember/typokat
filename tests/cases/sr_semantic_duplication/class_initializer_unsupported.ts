// Semantic-duplication architecture gate — representative unsupported initializer boundaries.
// tsc 6.0.3 --strict reports TS2304 for the unresolved name, TS2729/TS7022 for the forward/self
// references, and TS2695 for the sequence control; the other initializers are clean. typokat records
// one initializer-inference origin per field, while its later body walk independently retains TK2304
// and the existing child-slot incompletes for object spread, array spread, and computed object keys.

const initializerLocal = 1;
const initializerKey = "value";
const initializerObject = { value: 1 };
const initializerArray = [1];

class InitializerConstructedValue {
  value!: number;
}

class UnsupportedInitializerMatrix {
  earlier = 1;
  objectSpread = { ...initializerObject }; // incomplete[class/property-definition/initializer-inference]: unannotated field initializer cannot be inferred during class surface construction | incomplete[expr-infer/object-literal/spread-element]: object spread element not visited
  arraySpread = [...initializerArray]; // incomplete[class/property-definition/initializer-inference]: unannotated field initializer cannot be inferred during class surface construction | incomplete[expr-infer/array-literal/spread-element]: array spread element not visited
  computedObjectKey = { [initializerKey]: 1 }; // incomplete[class/property-definition/initializer-inference]: unannotated field initializer cannot be inferred during class surface construction | incomplete[expr-infer/object-literal/computed-key]: computed object key not visited
  localIdentifier = initializerLocal; // incomplete[class/property-definition/initializer-inference]: unannotated field initializer cannot be inferred during class surface construction
  unresolvedExternalIdentifier = MissingInitializerValue; // error[TK2304]: Cannot find name 'MissingInitializerValue' | incomplete[class/property-definition/initializer-inference]: unannotated field initializer cannot be inferred during class surface construction
  forwardThis = this.later; // incomplete[class/property-definition/initializer-inference]: unannotated field initializer cannot be inferred during class surface construction
  selfThis = this.selfThis; // incomplete[class/property-definition/initializer-inference]: unannotated field initializer cannot be inferred during class surface construction
  overloadCall = this.overloaded(); // incomplete[class/property-definition/initializer-inference]: unannotated field initializer cannot be inferred during class surface construction
  genericCall = this.generic<number>(); // incomplete[class/property-definition/initializer-inference]: unannotated field initializer cannot be inferred during class surface construction
  argumentCall = this.accepts(1); // incomplete[class/property-definition/initializer-inference]: unannotated field initializer cannot be inferred during class surface construction
  constructed = new InitializerConstructedValue(); // incomplete[class/property-definition/initializer-inference]: unannotated field initializer cannot be inferred during class surface construction
  blockArrow = () => { return 1; }; // incomplete[class/property-definition/initializer-inference]: unannotated field initializer cannot be inferred during class surface construction
  genericArrow = <T>(value: T) => value; // incomplete[class/property-definition/initializer-inference]: unannotated field initializer cannot be inferred during class surface construction
  memberAccess = initializerObject.value; // incomplete[class/property-definition/initializer-inference]: unannotated field initializer cannot be inferred during class surface construction
  elementAccess = initializerArray[0]; // incomplete[class/property-definition/initializer-inference]: unannotated field initializer cannot be inferred during class surface construction
  logical = true && 1; // incomplete[class/property-definition/initializer-inference]: unannotated field initializer cannot be inferred during class surface construction
  conditional = true ? 1 : 2; // incomplete[class/property-definition/initializer-inference]: unannotated field initializer cannot be inferred during class surface construction
  sequence = (0, 1); // incomplete[class/property-definition/initializer-inference]: unannotated field initializer cannot be inferred during class surface construction
  static staticLiteral = 1; // incomplete[class/property-definition/initializer-inference]: unannotated field initializer cannot be inferred during class surface construction
  static staticThis = this.staticLiteral; // incomplete[class/property-definition/initializer-inference]: unannotated field initializer cannot be inferred during class surface construction
  later = 2;

  overloaded(): number;
  overloaded(value: string): string;
  overloaded(value?: string): number | string {
    return value === undefined ? 1 : value;
  }

  generic<T>(): T {
    throw 0;
  }

  accepts(value: number): number {
    return value;
  }
}

// The two same-line origins are a marker multiset; exact internal order and spans are a Rust gate.
class SameLineUnsupportedInitializers { first = initializerLocal; second = initializerLocal; } // incomplete[class/property-definition/initializer-inference]: unannotated field initializer cannot be inferred during class surface construction | incomplete[class/property-definition/initializer-inference]: unannotated field initializer cannot be inferred during class surface construction
