declare function scalarIdentity<T>(value: T): T;
declare function scalarBox<T>(value: T): { value: T };

const ordinaryText = "read";
const directText = scalarIdentity(ordinaryText);
const exactDirectText: "read" = directText;
let copiedText = directText;
copiedText = "write";
const boxedText = scalarBox(ordinaryText);
const exactBoxedText: "read" = boxedText.value; // error[TK2322]
boxedText.value = "write";

const ordinaryNumber = 1;
const directNumber = scalarIdentity(ordinaryNumber);
const exactDirectNumber: 1 = directNumber;
let copiedNumber = directNumber;
copiedNumber = 2;
const boxedNumber = scalarBox(ordinaryNumber);
const exactBoxedNumber: 1 = boxedNumber.value; // error[TK2322]
boxedNumber.value = 2;

enum ScalarNumericMode { Read, Write }
const ordinaryNumericMode = ScalarNumericMode.Read;
const directNumericMode = scalarIdentity(ordinaryNumericMode);
const exactDirectNumericMode: ScalarNumericMode.Read = directNumericMode;
let copiedNumericMode = directNumericMode;
copiedNumericMode = ScalarNumericMode.Write;
const boxedNumericMode = scalarBox(ordinaryNumericMode);
const exactBoxedNumericMode: ScalarNumericMode.Read = boxedNumericMode.value; // error[TK2322]
boxedNumericMode.value = ScalarNumericMode.Write;

enum ScalarStringMode { Read = "read", Write = "write" }
const ordinaryStringMode = ScalarStringMode.Read;
const directStringMode = scalarIdentity(ordinaryStringMode);
const exactDirectStringMode: ScalarStringMode.Read = directStringMode;
let copiedStringMode = directStringMode;
copiedStringMode = ScalarStringMode.Write;
const boxedStringMode = scalarBox(ordinaryStringMode);
const exactBoxedStringMode: ScalarStringMode.Read = boxedStringMode.value; // error[TK2322]
boxedStringMode.value = ScalarStringMode.Write;

const regularText = "read" as const;
const regularDirectText = scalarIdentity(regularText);
const exactRegularText: "read" = regularDirectText;
let regularTextCopy = regularDirectText;
regularTextCopy = "write"; // error[TK2322]
const regularTextBox = scalarBox(regularText);
regularTextBox.value = "write"; // error[TK2322]

const regularNumber = 1 as const;
let regularNumberCopy = scalarIdentity(regularNumber);
regularNumberCopy = 2; // error[TK2322]
const regularNumberBox = scalarBox(regularNumber);
regularNumberBox.value = 2; // error[TK2322]

const regularNumericMode = ScalarNumericMode.Read as const;
let regularNumericCopy = scalarIdentity(regularNumericMode);
regularNumericCopy = ScalarNumericMode.Write; // error[TK2322]
const regularNumericBox = scalarBox(regularNumericMode);
regularNumericBox.value = ScalarNumericMode.Write; // error[TK2322]

const regularStringMode = ScalarStringMode.Read as const;
let regularStringCopy = scalarIdentity(regularStringMode);
regularStringCopy = ScalarStringMode.Write; // error[TK2322]
const regularStringBox = scalarBox(regularStringMode);
regularStringBox.value = ScalarStringMode.Write; // error[TK2322]
