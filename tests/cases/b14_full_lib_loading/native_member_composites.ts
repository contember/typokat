// tsc 6.0.3 --strict --target es2025: TS2322 x6, TS2341, and TS2445 below.

const objectLiteral = { value: 1 };
const objectLiteralText: string = objectLiteral.toString();
const wrongObjectLiteralText: number = objectLiteral.toString(); // error[TK2322]: Type 'string' is not assignable to type 'number'
const objectLiteralValue: number = objectLiteral.value;
const wrongObjectLiteralValue: string = objectLiteral.value; // error[TK2322]: Type 'number' is not assignable to type 'string'
const objectOverride = { toString: () => 1 };
const objectOverrideValue: number = objectOverride.toString();
const wrongObjectOverrideValue: string = objectOverride.toString(); // error[TK2322]: Type 'number' is not assignable to type 'string'

declare const primitiveUnion: string | number;
const primitiveUnionText: string = primitiveUnion.toString();
const wrongPrimitiveUnionText: number = primitiveUnion.toString(); // error[TK2322]: Type 'string' is not assignable to type 'number'
const primitiveUnionValue: string | number = primitiveUnion.valueOf();
const wrongPrimitiveUnionValue: boolean = primitiveUnion.valueOf(); // error[TK2322]

declare const decoratedString: string & { marker: 1 };
const decoratedUpper: string = decoratedString.toUpperCase();
const decoratedMarker: 1 = decoratedString.marker;
const wrongDecoratedMarker: 2 = decoratedString.marker; // error[TK2322]

class NativeCompositeAccess {
  private secret = 1;
  protected inherited = 2;
  visible = 3;
}
declare const decoratedAccess: NativeCompositeAccess & string;
decoratedAccess.secret; // error[TK2341]: Property 'secret' is private
decoratedAccess.inherited; // error[TK2445]: Property 'inherited' is protected
const decoratedVisible: number = decoratedAccess.visible;
