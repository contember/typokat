// The proof concatenates this file after the pinned lib so it exercises one type universe.

declare const wu6DeepArraySource: Array<string>;
const wu6DeepArrayReject: number = wu6DeepArraySource[0]; // witness[deep.Array.element] oracle[TS2322] typokat[TK2322]

declare const wu6DeepDateSource: Date;
const wu6DeepDateReject: string = wu6DeepDateSource.getTime(); // witness[deep.Date.member] oracle[TS2322] typokat[TK2322]

declare const wu6DeepIntlTypeSource: Intl.CollatorOptions;
const wu6DeepIntlTypeReject: number = wu6DeepIntlTypeSource.usage; // witness[deep.Intl.type] oracle[TS2322] typokat[TK2322]
const wu6DeepIntlValueReject: string = Intl.Collator().compare("a", "b"); // witness[deep.Intl.value] oracle[TS2322] typokat[TK2304] owner[../../../docs/backlog/43-namespaces-declaration-merging.md]

declare const wu6DeepNumberSource: Number;
const wu6DeepNumberReject: number = wu6DeepNumberSource.toFixed(); // witness[deep.Number.member] oracle[TS2322] typokat[TK2322]

declare const wu6DeepObjectSource: Object;
const wu6DeepObjectReject: number = wu6DeepObjectSource.toString(); // witness[deep.Object.member] oracle[TS2322] typokat[TK2322]

declare const wu6DeepStringSource: String;
const wu6DeepStringReject: number = wu6DeepStringSource.charAt(0); // witness[deep.String.member] oracle[TS2322] typokat[TK2322]

declare const wu6DeepRepeatedDateSource: Date;
const wu6DeepRepeatedDateReject: number = wu6DeepRepeatedDateSource.toLocaleDateString(); // witness[deep.repeat.Date] oracle[TS2322] typokat[TK2322]

declare const wu6DeepRepeatedNumberSource: Number;
const wu6DeepRepeatedNumberReject: number = wu6DeepRepeatedNumberSource.toLocaleString(); // witness[deep.repeat.Number] oracle[TS2322] typokat[TK2322]

declare const wu6DeepRepeatedStringSource: String;
const wu6DeepRepeatedStringReject: string = wu6DeepRepeatedStringSource.localeCompare("a"); // witness[deep.repeat.String] oracle[TS2322] typokat[TK2322]

declare const wu6PairArrayTypeSource: Array<string>;
const wu6PairArrayTypeReject: number = wu6PairArrayTypeSource; // witness[pair.Array.type] oracle[TS2322] typokat[TK2322]
const wu6PairArrayValueReject: number = Array; // witness[pair.Array.value] oracle[TS2322] typokat[TK2322]

declare const wu6PairArrayBufferTypeSource: ArrayBuffer;
const wu6PairArrayBufferTypeReject: number = wu6PairArrayBufferTypeSource; // witness[pair.ArrayBuffer.type] oracle[TS2322] typokat[TK2322]
const wu6PairArrayBufferValueReject: number = ArrayBuffer; // witness[pair.ArrayBuffer.value] oracle[TS2322] typokat[TK2322]

declare const wu6PairBooleanTypeSource: Boolean;
const wu6PairBooleanTypeReject: number = wu6PairBooleanTypeSource; // witness[pair.Boolean.type] oracle[TS2322] typokat[TK2322]
const wu6PairBooleanValueReject: number = Boolean; // witness[pair.Boolean.value] oracle[TS2322] typokat[TK2322]

declare const wu6PairDataViewTypeSource: DataView<ArrayBuffer>;
const wu6PairDataViewTypeReject: number = wu6PairDataViewTypeSource; // witness[pair.DataView.type] oracle[TS2322] typokat[TK2322]
const wu6PairDataViewValueReject: number = DataView; // witness[pair.DataView.value] oracle[TS2322] typokat[TK2322]

declare const wu6PairDateTypeSource: Date;
const wu6PairDateTypeReject: number = wu6PairDateTypeSource; // witness[pair.Date.type] oracle[TS2322] typokat[TK2322]
const wu6PairDateValueReject: number = Date; // witness[pair.Date.value] oracle[TS2322] typokat[TK2322]

declare const wu6PairErrorTypeSource: Error;
const wu6PairErrorTypeReject: number = wu6PairErrorTypeSource; // witness[pair.Error.type] oracle[TS2322] typokat[TK2322]
const wu6PairErrorValueReject: number = Error; // witness[pair.Error.value] oracle[TS2322] typokat[TK2322]

declare const wu6PairEvalErrorTypeSource: EvalError;
const wu6PairEvalErrorTypeReject: number = wu6PairEvalErrorTypeSource; // witness[pair.EvalError.type] oracle[TS2322] typokat[TK2322]
const wu6PairEvalErrorValueReject: number = EvalError; // witness[pair.EvalError.value] oracle[TS2322] typokat[TK2322]

declare const wu6PairFloat32ArrayTypeSource: Float32Array<ArrayBuffer>;
const wu6PairFloat32ArrayTypeReject: number = wu6PairFloat32ArrayTypeSource; // witness[pair.Float32Array.type] oracle[TS2322] typokat[TK2322]
const wu6PairFloat32ArrayValueReject: number = Float32Array; // witness[pair.Float32Array.value] oracle[TS2322] typokat[TK2322]

declare const wu6PairFloat64ArrayTypeSource: Float64Array<ArrayBuffer>;
const wu6PairFloat64ArrayTypeReject: number = wu6PairFloat64ArrayTypeSource; // witness[pair.Float64Array.type] oracle[TS2322] typokat[TK2322]
const wu6PairFloat64ArrayValueReject: number = Float64Array; // witness[pair.Float64Array.value] oracle[TS2322] typokat[TK2322]

declare const wu6PairFunctionTypeSource: Function;
const wu6PairFunctionTypeReject: number = wu6PairFunctionTypeSource; // witness[pair.Function.type] oracle[TS2322] typokat[TK2322]
const wu6PairFunctionValueReject: number = Function; // witness[pair.Function.value] oracle[TS2322] typokat[TK2322]

declare const wu6PairInt16ArrayTypeSource: Int16Array<ArrayBuffer>;
const wu6PairInt16ArrayTypeReject: number = wu6PairInt16ArrayTypeSource; // witness[pair.Int16Array.type] oracle[TS2322] typokat[TK2322]
const wu6PairInt16ArrayValueReject: number = Int16Array; // witness[pair.Int16Array.value] oracle[TS2322] typokat[TK2322]

declare const wu6PairInt32ArrayTypeSource: Int32Array<ArrayBuffer>;
const wu6PairInt32ArrayTypeReject: number = wu6PairInt32ArrayTypeSource; // witness[pair.Int32Array.type] oracle[TS2322] typokat[TK2322]
const wu6PairInt32ArrayValueReject: number = Int32Array; // witness[pair.Int32Array.value] oracle[TS2322] typokat[TK2322]

declare const wu6PairInt8ArrayTypeSource: Int8Array<ArrayBuffer>;
const wu6PairInt8ArrayTypeReject: number = wu6PairInt8ArrayTypeSource; // witness[pair.Int8Array.type] oracle[TS2322] typokat[TK2322]
const wu6PairInt8ArrayValueReject: number = Int8Array; // witness[pair.Int8Array.value] oracle[TS2322] typokat[TK2322]

declare const wu6PairJSONTypeSource: JSON;
const wu6PairJSONTypeReject: number = wu6PairJSONTypeSource; // witness[pair.JSON.type] oracle[TS2322] typokat[TK2322]
const wu6PairJSONValueReject: number = JSON; // witness[pair.JSON.value] oracle[TS2322] typokat[TK2322]

declare const wu6PairMathTypeSource: Math;
const wu6PairMathTypeReject: number = wu6PairMathTypeSource; // witness[pair.Math.type] oracle[TS2322] typokat[TK2322]
const wu6PairMathValueReject: number = Math; // witness[pair.Math.value] oracle[TS2322] typokat[TK2322]

declare const wu6PairNumberTypeSource: Number;
const wu6PairNumberTypeReject: number = wu6PairNumberTypeSource; // witness[pair.Number.type] oracle[TS2322] typokat[TK2322]
const wu6PairNumberValueReject: number = Number; // witness[pair.Number.value] oracle[TS2322] typokat[TK2322]

declare const wu6PairObjectTypeSource: Object;
const wu6PairObjectTypeReject: number = wu6PairObjectTypeSource; // witness[pair.Object.type] oracle[TS2322] typokat[TK2322]
const wu6PairObjectValueReject: number = Object; // witness[pair.Object.value] oracle[TS2322] typokat[TK2322]

declare const wu6PairRangeErrorTypeSource: RangeError;
const wu6PairRangeErrorTypeReject: number = wu6PairRangeErrorTypeSource; // witness[pair.RangeError.type] oracle[TS2322] typokat[TK2322]
const wu6PairRangeErrorValueReject: number = RangeError; // witness[pair.RangeError.value] oracle[TS2322] typokat[TK2322]

declare const wu6PairReferenceErrorTypeSource: ReferenceError;
const wu6PairReferenceErrorTypeReject: number = wu6PairReferenceErrorTypeSource; // witness[pair.ReferenceError.type] oracle[TS2322] typokat[TK2322]
const wu6PairReferenceErrorValueReject: number = ReferenceError; // witness[pair.ReferenceError.value] oracle[TS2322] typokat[TK2322]

declare const wu6PairRegExpTypeSource: RegExp;
const wu6PairRegExpTypeReject: number = wu6PairRegExpTypeSource; // witness[pair.RegExp.type] oracle[TS2322] typokat[TK2322]
const wu6PairRegExpValueReject: number = RegExp; // witness[pair.RegExp.value] oracle[TS2322] typokat[TK2322]

declare const wu6PairStringTypeSource: String;
const wu6PairStringTypeReject: number = wu6PairStringTypeSource; // witness[pair.String.type] oracle[TS2322] typokat[TK2322]
const wu6PairStringValueReject: number = String; // witness[pair.String.value] oracle[TS2322] typokat[TK2322]

declare const wu6PairSyntaxErrorTypeSource: SyntaxError;
const wu6PairSyntaxErrorTypeReject: number = wu6PairSyntaxErrorTypeSource; // witness[pair.SyntaxError.type] oracle[TS2322] typokat[TK2322]
const wu6PairSyntaxErrorValueReject: number = SyntaxError; // witness[pair.SyntaxError.value] oracle[TS2322] typokat[TK2322]

declare const wu6PairTypeErrorTypeSource: TypeError;
const wu6PairTypeErrorTypeReject: number = wu6PairTypeErrorTypeSource; // witness[pair.TypeError.type] oracle[TS2322] typokat[TK2322]
const wu6PairTypeErrorValueReject: number = TypeError; // witness[pair.TypeError.value] oracle[TS2322] typokat[TK2322]

declare const wu6PairURIErrorTypeSource: URIError;
const wu6PairURIErrorTypeReject: number = wu6PairURIErrorTypeSource; // witness[pair.URIError.type] oracle[TS2322] typokat[TK2322]
const wu6PairURIErrorValueReject: number = URIError; // witness[pair.URIError.value] oracle[TS2322] typokat[TK2322]

declare const wu6PairUint16ArrayTypeSource: Uint16Array<ArrayBuffer>;
const wu6PairUint16ArrayTypeReject: number = wu6PairUint16ArrayTypeSource; // witness[pair.Uint16Array.type] oracle[TS2322] typokat[TK2322]
const wu6PairUint16ArrayValueReject: number = Uint16Array; // witness[pair.Uint16Array.value] oracle[TS2322] typokat[TK2322]

declare const wu6PairUint32ArrayTypeSource: Uint32Array<ArrayBuffer>;
const wu6PairUint32ArrayTypeReject: number = wu6PairUint32ArrayTypeSource; // witness[pair.Uint32Array.type] oracle[TS2322] typokat[TK2322]
const wu6PairUint32ArrayValueReject: number = Uint32Array; // witness[pair.Uint32Array.value] oracle[TS2322] typokat[TK2322]

declare const wu6PairUint8ArrayTypeSource: Uint8Array<ArrayBuffer>;
const wu6PairUint8ArrayTypeReject: number = wu6PairUint8ArrayTypeSource; // witness[pair.Uint8Array.type] oracle[TS2322] typokat[TK2322]
const wu6PairUint8ArrayValueReject: number = Uint8Array; // witness[pair.Uint8Array.value] oracle[TS2322] typokat[TK2322]

declare const wu6PairUint8ClampedArrayTypeSource: Uint8ClampedArray<ArrayBuffer>;
const wu6PairUint8ClampedArrayTypeReject: number = wu6PairUint8ClampedArrayTypeSource; // witness[pair.Uint8ClampedArray.type] oracle[TS2322] typokat[TK2322]
const wu6PairUint8ClampedArrayValueReject: number = Uint8ClampedArray; // witness[pair.Uint8ClampedArray.value] oracle[TS2322] typokat[TK2322]
