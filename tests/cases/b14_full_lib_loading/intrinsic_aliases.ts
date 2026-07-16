// tsc 6.0.3 --strict --target es2025: TS2322 x6 and TS2339 below. WU0 has not
// admitted NoInfer or Awaited as terminal library semantics.

type Loud = Uppercase<"quiet">;
const loud: Loud = "QUIET";
const wrongLoud: Loud = "quiet"; // error[TK2322]

type Quiet = Lowercase<"LOUD">;
const quiet: Quiet = "loud";
const wrongQuiet: Quiet = "LOUD"; // error[TK2322]

type Capitalized = Capitalize<"typokat">;
const capitalized: Capitalized = "Typokat";
const wrongCapitalized: Capitalized = "typokat"; // error[TK2322]

type Uncapitalized = Uncapitalize<"Typokat">;
const uncapitalized: Uncapitalized = "typokat";
const wrongUncapitalized: Uncapitalized = "Typokat"; // error[TK2322]

type NumberReturn = ReturnType<(value: string) => number>;
const returned: NumberReturn = 1;
const wrongReturned: NumberReturn = "one"; // error[TK2322]: Type 'string' is not assignable to type 'number'

declare const receiverFunction: (this: { prefix: string }, value: string) => string;
const detached: OmitThisParameter<(this: { prefix: string }, value: string) => string> = receiverFunction;
const detachedResult: string = detached("value");
const wrongDetached: number = detached("value"); // error[TK2322]: Type 'string' is not assignable to type 'number'

type Descriptor<Data, Methods> = {
  data: Data;
  methods: Methods & ThisType<Data & Methods>;
};

const descriptor: Descriptor<{ count: number }, { increment(): number }> = {
  data: { count: 1 },
  methods: {
    increment() {
      return this.count + 1;
    },
  },
};

const wrongDescriptor: Descriptor<{ count: number }, { increment(): number }> = {
  data: { count: 1 },
  methods: {
    increment() {
      return this.missing; // error[TK2339]
    },
  },
};
