// b06: TK2654 — TWO OR MORE missing abstract members aggregate into one diagnostic
// on the class name: quoted member names, direct base's own abstract members first,
// then inherited pending ones (tsc 6.0.3 order, probed).

abstract class AB {
  abstract go(): number;
  abstract val: string;
}

class Impl2 extends AB {} // error[TK2654]: Non-abstract class 'Impl2' is missing implementations for the following members of 'AB': 'go', 'val'

class PartialImpl extends AB { // error[TK2515]: Non-abstract class 'PartialImpl' does not implement inherited abstract member go from class 'AB'
  val: string = "x";
}

abstract class ABC extends AB {
  abstract third(): boolean;
}

class Impl3 extends ABC {} // error[TK2654]: Non-abstract class 'Impl3' is missing implementations for the following members of 'ABC': 'third', 'go', 'val'
