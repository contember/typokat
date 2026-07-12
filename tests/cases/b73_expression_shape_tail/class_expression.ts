// tsc 6.0.3 --strict: clean; local class-expression checking remains deferred.

const C = class {}; // incomplete[expr-infer/class-expression/self]
