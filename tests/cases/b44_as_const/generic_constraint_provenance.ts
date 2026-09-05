// Const-preserving inference candidates still satisfy dependent generic constraints.

declare function dependent<T, U extends T>(base: T, child: U): void;

dependent(1 as const, 2 as const); // error[TK2345]: Argument of type '2' is not assignable to parameter of type '1'
