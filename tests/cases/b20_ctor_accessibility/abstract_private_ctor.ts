// b20: when a class is BOTH abstract and has an inaccessible constructor, the
// accessibility error wins and TK2511 is suppressed (tsc 6.0.3 reports only
// TS2673 there). A plain abstract class still reports TK2511.

abstract class AbsPriv {
  private constructor() {}
}

new AbsPriv(); // error[TK2673]: Constructor of class 'AbsPriv' is private and only accessible within the class declaration

abstract class AbsPub {
  constructor() {}
}

new AbsPub(); // error[TK2511]: Cannot create an instance of an abstract class
