// tsc 6.0.3 --strict --lib es5 --noEmit: inherited same-kind index conflicts
// use TS2430 at the derived header; cross-family conflicts use TS2411/TS2413.

interface StringIndexText { [key: string]: string }
interface StringIndexNumber { [key: string]: number }
interface StringIndexTextFirst extends StringIndexText, StringIndexNumber {} // error[TK2430]: Interface 'StringIndexTextFirst' incorrectly extends interface 'StringIndexNumber'
interface StringIndexNumberFirst extends StringIndexNumber, StringIndexText {} // error[TK2430]: Interface 'StringIndexNumberFirst' incorrectly extends interface 'StringIndexText'

interface NumberIndexText { [key: number]: string }
interface NumberIndexNumber { [key: number]: number }
interface NumberIndexTextFirst extends NumberIndexText, NumberIndexNumber {} // error[TK2430]: Interface 'NumberIndexTextFirst' incorrectly extends interface 'NumberIndexNumber'
interface NumberIndexNumberFirst extends NumberIndexNumber, NumberIndexText {} // error[TK2430]: Interface 'NumberIndexNumberFirst' incorrectly extends interface 'NumberIndexText'

interface NamedPropertyBase { fixed: string }
interface CrossStringIndexBase { [key: string]: number }
interface InheritedPropertyIndex extends NamedPropertyBase, CrossStringIndexBase {} // error[TK2411]: Property 'fixed' of type 'string' is not assignable to 'string' index type 'number'
interface InheritedIndexProperty extends CrossStringIndexBase, NamedPropertyBase {} // error[TK2411]: Property 'fixed' of type 'string' is not assignable to 'string' index type 'number'

interface InheritedNumberIndexBase { [key: number]: string }
interface InheritedStringIndexBase { [key: string]: number }
interface InheritedNumberString extends InheritedNumberIndexBase, InheritedStringIndexBase {} // error[TK2413]: 'number' index type 'string' is not assignable to 'string' index type 'number'
interface InheritedStringNumber extends InheritedStringIndexBase, InheritedNumberIndexBase {} // error[TK2413]: 'number' index type 'string' is not assignable to 'string' index type 'number'

interface CompatibleStringIndexA { [key: string]: string }
interface CompatibleStringIndexB { [key: string]: string }
interface CompatibleStringIndices extends CompatibleStringIndexA, CompatibleStringIndexB {}

interface CompatibleMixedStringIndex { [key: string]: unknown }
interface CompatibleMixedNumberIndex { [key: number]: number }
interface CompatibleMixedIndices extends CompatibleMixedStringIndex, CompatibleMixedNumberIndex {}

// Own overlays are checked against every base. Same-kind incompatibility stays
// header-owned; cross-family conflicts belong to the own property/index occurrence.
interface OwnOverlayStringBase { [key: string]: string }
interface OwnOverlayNumberBase { [key: string]: number }
interface OwnOverlayIndex extends OwnOverlayStringBase, OwnOverlayNumberBase { // error[TK2430]: Interface 'OwnOverlayIndex' incorrectly extends interface 'OwnOverlayNumberBase' | error[TK2430]: Interface 'OwnOverlayIndex' incorrectly extends interface 'OwnOverlayStringBase'
  [key: string]: boolean;
}

interface OwnPropertyTypeBase { fixed: string }
interface OwnPropertyTypeDerived extends OwnPropertyTypeBase { // error[TK2430]: Interface 'OwnPropertyTypeDerived' incorrectly extends interface 'OwnPropertyTypeBase'
  fixed: number;
}

interface OwnPropertyIndexBase { [key: string]: number }
interface OwnPropertyAgainstIndex extends OwnPropertyIndexBase {
  fixed: string; // error[TK2411]: Property 'fixed' of type 'string' is not assignable to 'string' index type 'number'
}

interface OwnIndexPropertyBase { fixed: string }
interface OwnIndexAgainstProperty extends OwnIndexPropertyBase {
  [key: string]: number; // error[TK2411]: Property 'fixed' of type 'string' is not assignable to 'string' index type 'number'
}

interface OwnNumberStringBase { [key: string]: number }
interface OwnNumberAgainstString extends OwnNumberStringBase {
  [key: number]: string; // error[TK2413]: 'number' index type 'string' is not assignable to 'string' index type 'number'
}

interface OwnStringNumberBase { [key: number]: string }
interface OwnStringAgainstNumber extends OwnStringNumberBase {
  [key: string]: number; // error[TK2413]: 'number' index type 'string' is not assignable to 'string' index type 'number'
}

// Differing call signatures are accumulated overloads, not heritage conflicts.
interface CallStringBase { (value: string): string }
interface CallNumberBase { (value: number): number }
interface DifferingCallSignatures extends CallStringBase, CallNumberBase {}
declare const differingCallSignatures: DifferingCallSignatures;
const calledWithString: string = differingCallSignatures("ok");
const calledWithNumber: number = differingCallSignatures(1);
