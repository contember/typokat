// Semantic-duplication sprint WU0 — class callable surfaces are reserved once.
// Cross-checked with tsc 6.0.3 --strict. All marked cases have tsc parity except
// the final documented external-void pair, which intentionally preserves current typokat behavior.

class GenericMethodSurface {
  convert<T extends string, U extends T = T>(value: U): T {
    return value;
  }
}

const genericSurface = new GenericMethodSurface();
const inferredGeneric: string = genericSurface.convert("value");
const defaultedGeneric: "ok" = genericSurface.convert<"ok">("ok");
genericSurface.convert<"ok">("bad"); // error[TK2345]: Argument of type '"bad"' is not assignable to parameter of type '"ok"'
genericSurface.convert<number>(1); // error[TK2344]: Type 'number' does not satisfy the constraint 'string'

// Exactly four signature references and one body reference belong to this static-method source.
class StaticBinderSurface<T> {
  static capture<
    U extends T = T, // error[TK2302]: Static members cannot reference class type parameters | error[TK2302]: Static members cannot reference class type parameters
  >(
    value: T, // error[TK2302]: Static members cannot reference class type parameters
  ): T { // error[TK2302]: Static members cannot reference class type parameters
    const bodyValue: T = value; // error[TK2302]: Static members cannot reference class type parameters
    return bodyValue;
  }
}

// Reservation owns these annotations: body checking must not emit them a second time.
class UnresolvedMethodSurface {
  method(
    value: MissingMethodParameter, // error[TK2304]: Cannot find name 'MissingMethodParameter'
  ): MissingMethodReturn { // error[TK2304]: Cannot find name 'MissingMethodReturn'
    return value;
  }
}

class InvalidMethodDefaultSurface {
  method<T extends string = number>(): T { // error[TK2344]: Type 'number' does not satisfy the constraint 'string'
    throw 0;
  }
}

// Only the first incompatible overload is diagnosed. The implementation signature stays hidden.
class MethodOverloadSurface {
  method(value: string): string; // error[TK2394]: not compatible with its implementation signature
  method(value: number): number;
  method(value: boolean): boolean { return value; }
}

const methodOverloads = new MethodOverloadSurface();
methodOverloads.method(true); // error[TK2769]: No overload matches this call

class ConstructorOverloadSurface {
  constructor(value: string); // error[TK2394]: not compatible with its implementation signature
  constructor(value: number);
  constructor(value: boolean) {}
}

new ConstructorOverloadSurface(true); // error[TK2769]: No overload matches this call

// Parameter properties reuse their reserved parameter types. Readonly writes remain constructor-only.
class ParameterPropertySurface {
  constructor(public value: number, readonly frozen: string) {
    const parameterValue: number = value;
    const parameterFrozen: string = frozen;
    this.value = parameterValue;
    this.frozen = parameterFrozen;
  }

  update(next: number): void {
    this.value = next;
    this.frozen = "blocked"; // error[TK2540]: Cannot assign to 'frozen' because it is a read-only property
  }
}

const parameterProperties = new ParameterPropertySurface(1, "fixed");
const publicParameterProperty: number = parameterProperties.value;
const readonlyParameterProperty: string = parameterProperties.frozen;

class MissingParameterPropertySurface {
  constructor(public missing: AbsentParameterPropertyType) {} // error[TK2304]: Cannot find name 'AbsentParameterPropertyType'
}

// Deliberate backlog-76 pair: class fill currently publishes an omitted method return as void.
class ExternalVoidSurface {
  value() {
    return 1;
  }
}

// tsc is clean (the body-inferred return is number); typokat deliberately over-reports for now.
const externalAsNumber: number = new ExternalVoidSurface().value(); // error[TK2322]: Type 'void' is not assignable to type 'number'
// tsc reports TS2322 (number -> void); typokat deliberately remains clean until backlog 76.
const externalAsVoid: void = new ExternalVoidSurface().value();
