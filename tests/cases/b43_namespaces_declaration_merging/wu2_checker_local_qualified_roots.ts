// tsc 6.0.3 --strict: TS2702 x6 and TS2503; four namespace-precedence controls are clean.
declare function functionTypeParameterRoot<FunctionT>(): FunctionT.Member; // error[TK2702]: 'FunctionT' only refers to a type, but is being used as a namespace here

interface MethodTypeParameterRoot {
  method<MethodT>(): MethodT.Member; // error[TK2702]: 'MethodT' only refers to a type, but is being used as a namespace here
}

type ConditionalInferRoot<Source> = Source extends infer InferT ? InferT.Member : never; // error[TK2702]: 'InferT' only refers to a type, but is being used as a namespace here

type MappedBinderRoot<Source> = {
  [MappedK in keyof Source]: MappedK.Member; // error[TK2702]: 'MappedK' only refers to a type, but is being used as a namespace here
};

type BuiltinRoot = Array.Member; // error[TK2702]: 'Array' only refers to a type, but is being used as a namespace here

abstract class ClassTypeParameterRoot<ClassT> {
  abstract field: ClassT.Member; // error[TK2702]: 'ClassT' only refers to a type, but is being used as a namespace here
}

class StaticClassTypeParameterRoot<StaticT> {
  static field: StaticT.Member; // error[TK2503]: Cannot find namespace 'StaticT'
}

namespace T {
  export interface Member { typeParameterNamespace: true }
}
declare function namespacePrecedesTypeParameter<T>(): T.Member;

namespace U {
  export interface Member { inferNamespace: true }
}
type NamespacePrecedesInfer<Source> = Source extends infer U ? U.Member : never;

namespace K {
  export interface Member { mappedNamespace: true }
}
type NamespacePrecedesMapped<Source> = {
  [K in keyof Source]: K.Member;
};

namespace BuiltinHost {
  export namespace Array {
    export interface Member { nestedBuiltinNamespace: true }
  }
  export type NestedBuiltinNamespace = Array.Member;
}
