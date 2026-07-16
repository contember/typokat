// WU4 — tsc 6.0.3 --strict --noEmit --lib es5 --module commonjs:
// TS2300 x4, TS2434 x2, TS2345 x4, TS2322 x18, and TS2339 below. Explicit field,
// static, namespace-value, and private annotations keep unrelated inference outside this fixture's scope.

class Wu4ForwardClass {
  static existing: number = 1;
  private identity: number = 1;
  instance: number;

  constructor(value: number) {
    this.instance = value;
  }
}
namespace Wu4ForwardClass {
  const hidden: boolean = true;
  export const tag: string = "forward";
  export interface Options {
    enabled: boolean;
  }
}

const wu4ForwardClassInstance = new Wu4ForwardClass(1);
const wu4ForwardClassInstanceValue: number = wu4ForwardClassInstance.instance;
const wu4ForwardClassStatic: number = Wu4ForwardClass.existing;
const wu4ForwardClassTag: string = Wu4ForwardClass.tag;
declare const wu4ForwardClassOptions: Wu4ForwardClass.Options;
new Wu4ForwardClass("bad"); // error[TK2345]: Argument of type 'string' is not assignable to parameter of type 'number'
const wu4ForwardClassInstanceWrong: string = wu4ForwardClassInstance.instance; // error[TK2322]: Type 'number' is not assignable to type 'string'
const wu4ForwardClassStaticWrong: string = Wu4ForwardClass.existing; // error[TK2322]: Type 'number' is not assignable to type 'string'
const wu4ForwardClassTagWrong: number = Wu4ForwardClass.tag; // error[TK2322]: Type 'string' is not assignable to type 'number'
const wu4ForwardClassOptionsWrong: number = wu4ForwardClassOptions.enabled; // error[TK2322]: Type 'boolean' is not assignable to type 'number'
Wu4ForwardClass.hidden; // error[TK2339]: Property 'hidden' does not exist on type 'typeof Wu4ForwardClass'

namespace Wu4ReverseClass { // error[TK2434]: A namespace declaration cannot be located prior to a class or function with which it is merged
  export const tag: string = "reverse";
  export interface Options {
    enabled: boolean;
  }
}
class Wu4ReverseClass {
  static existing: number = 1;
  private identity: number = 1;
  instance: number;

  constructor(value: number) {
    this.instance = value;
  }
}

const wu4ReverseClassInstance = new Wu4ReverseClass(1);
const wu4ReverseClassInstanceValue: number = wu4ReverseClassInstance.instance;
const wu4ReverseClassStatic: number = Wu4ReverseClass.existing;
const wu4ReverseClassTag: string = Wu4ReverseClass.tag;
declare const wu4ReverseClassOptions: Wu4ReverseClass.Options;
new Wu4ReverseClass("bad"); // error[TK2345]: Argument of type 'string' is not assignable to parameter of type 'number'
const wu4ReverseClassInstanceWrong: string = wu4ReverseClassInstance.instance; // error[TK2322]: Type 'number' is not assignable to type 'string'
const wu4ReverseClassStaticWrong: string = Wu4ReverseClass.existing; // error[TK2322]: Type 'number' is not assignable to type 'string'
const wu4ReverseClassTagWrong: number = Wu4ReverseClass.tag; // error[TK2322]: Type 'string' is not assignable to type 'number'
const wu4ReverseClassOptionsWrong: number = wu4ReverseClassOptions.enabled; // error[TK2322]: Type 'boolean' is not assignable to type 'number'

declare class Wu4AmbientForwardClass {
  static existing: number;
  private identity: number;
  instance: number;
  constructor(value: number);
}
declare namespace Wu4AmbientForwardClass {
  const tag: string;
  interface Options {
    enabled: boolean;
  }
}

const wu4AmbientForwardClassInstance = new Wu4AmbientForwardClass(1);
const wu4AmbientForwardClassInstanceValue: number = wu4AmbientForwardClassInstance.instance;
const wu4AmbientForwardClassStatic: number = Wu4AmbientForwardClass.existing;
const wu4AmbientForwardClassTag: string = Wu4AmbientForwardClass.tag;
declare const wu4AmbientForwardClassOptions: Wu4AmbientForwardClass.Options;
new Wu4AmbientForwardClass("bad"); // error[TK2345]: Argument of type 'string' is not assignable to parameter of type 'number'
const wu4AmbientForwardClassInstanceWrong: string = wu4AmbientForwardClassInstance.instance; // error[TK2322]: Type 'number' is not assignable to type 'string'
const wu4AmbientForwardClassStaticWrong: string = Wu4AmbientForwardClass.existing; // error[TK2322]: Type 'number' is not assignable to type 'string'
const wu4AmbientForwardClassTagWrong: number = Wu4AmbientForwardClass.tag; // error[TK2322]: Type 'string' is not assignable to type 'number'
const wu4AmbientForwardClassOptionsWrong: number = wu4AmbientForwardClassOptions.enabled; // error[TK2322]: Type 'boolean' is not assignable to type 'number'

declare namespace Wu4AmbientReverseClass {
  const tag: string;
  interface Options {
    enabled: boolean;
  }
}
declare class Wu4AmbientReverseClass {
  static existing: number;
  private identity: number;
  instance: number;
  constructor(value: number);
}

const wu4AmbientReverseClassInstance = new Wu4AmbientReverseClass(1);
const wu4AmbientReverseClassInstanceValue: number = wu4AmbientReverseClassInstance.instance;
const wu4AmbientReverseClassStatic: number = Wu4AmbientReverseClass.existing;
const wu4AmbientReverseClassTag: string = Wu4AmbientReverseClass.tag;
declare const wu4AmbientReverseClassOptions: Wu4AmbientReverseClass.Options;
new Wu4AmbientReverseClass("bad"); // error[TK2345]: Argument of type 'string' is not assignable to parameter of type 'number'
const wu4AmbientReverseClassInstanceWrong: string = wu4AmbientReverseClassInstance.instance; // error[TK2322]: Type 'number' is not assignable to type 'string'
const wu4AmbientReverseClassStaticWrong: string = Wu4AmbientReverseClass.existing; // error[TK2322]: Type 'number' is not assignable to type 'string'
const wu4AmbientReverseClassTagWrong: number = Wu4AmbientReverseClass.tag; // error[TK2322]: Type 'string' is not assignable to type 'number'
const wu4AmbientReverseClassOptionsWrong: number = wu4AmbientReverseClassOptions.enabled; // error[TK2322]: Type 'boolean' is not assignable to type 'number'

class Wu4ForwardStaticCollision {
  static collision(): number { return 1; } // error[TK2300]: Duplicate identifier 'collision'
}
namespace Wu4ForwardStaticCollision {
  export function collision(): string { return "namespace"; } // error[TK2300]: Duplicate identifier 'collision'
}
const wu4ForwardStaticCollision: number = Wu4ForwardStaticCollision.collision();
const wu4ForwardStaticCollisionWrong: string = Wu4ForwardStaticCollision.collision(); // error[TK2322]: Type 'number' is not assignable to type 'string'

namespace Wu4ReverseStaticCollision { // error[TK2434]: A namespace declaration cannot be located prior to a class or function with which it is merged
  export function collision(): string { return "namespace"; } // error[TK2300]: Duplicate identifier 'collision'
}
class Wu4ReverseStaticCollision {
  static collision(): number { return 1; } // error[TK2300]: Duplicate identifier 'collision'
}
const wu4ReverseStaticCollision: string = Wu4ReverseStaticCollision.collision();
const wu4ReverseStaticCollisionWrong: number = Wu4ReverseStaticCollision.collision(); // error[TK2322]: Type 'string' is not assignable to type 'number'
