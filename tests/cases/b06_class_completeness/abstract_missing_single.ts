// b06: TK2515 — exactly ONE missing inherited abstract member. The message names
// the DIRECT base (not the declaring class) and the member name is unquoted,
// per tsc 6.0.3.

abstract class A {
  abstract go(): number;
}

class Impl extends A {} // error[TK2515]: Non-abstract class 'Impl' does not implement inherited abstract member go from class 'A'

abstract class Mid extends A {}

class Leaf extends Mid {} // error[TK2515]: Non-abstract class 'Leaf' does not implement inherited abstract member go from class 'Mid'

abstract class Mid2 extends A {
  go(): number { return 1; }
}

class Leaf2 extends Mid2 {}

class Good extends A {
  go(): number { return 2; }
}
