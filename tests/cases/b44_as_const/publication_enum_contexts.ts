enum PublicationNumber { A, B }
enum PublicationString { A = "a", B = "b" }
enum OtherPublicationNumber { A, B }

const numberBox = { value: PublicationNumber.A } satisfies { value: PublicationNumber.A };
const stringBox = { value: PublicationString.A } satisfies { value: PublicationString.A };
let numberCopy = numberBox.value;
numberCopy = PublicationNumber.B; // error[TK2322]
let stringCopy = stringBox.value;
stringCopy = PublicationString.B; // error[TK2322]

const numberElements = [PublicationNumber.A] satisfies PublicationNumber.A[];
let numberElementCopy = numberElements[0];
numberElementCopy = PublicationNumber.B; // error[TK2322]
const stringElements = [PublicationString.A] satisfies [PublicationString.A];
let stringElementCopy = stringElements[0];
stringElementCopy = PublicationString.B; // error[TK2322]

const ordinaryEnumSource = PublicationNumber.A;
const transportedBox = { value: ordinaryEnumSource } satisfies { value: PublicationNumber.A };
let transportedCopy = transportedBox.value;
transportedCopy = PublicationNumber.B; // error[TK2322]
const declaredBox: { value: PublicationString.A } = { value: PublicationString.A };
const exactDeclared: PublicationString.A = declaredBox.value;

const wholeEnumBox = { value: PublicationNumber.A } satisfies { value: PublicationNumber };
let wholeEnumCopy = wholeEnumBox.value;
wholeEnumCopy = PublicationNumber.B; // error[TK2322]
const wrongOwner: { value: OtherPublicationNumber.A } = { value: PublicationNumber.A }; // error[TK2322]
const wrongMember: { value: PublicationString.B } = { value: PublicationString.A }; // error[TK2322]

const broadNumberBox = { value: PublicationNumber.A } satisfies { value: number };
let broadNumberCopy = broadNumberBox.value;
broadNumberCopy = PublicationNumber.B;
const broadStringBox = { value: PublicationString.A } satisfies { value: string };
let broadStringCopy = broadStringBox.value;
broadStringCopy = PublicationString.B;

const numberReturn = (() => PublicationNumber.A) satisfies () => PublicationNumber.A;
let numberReturnCopy = numberReturn();
numberReturnCopy = PublicationNumber.B; // error[TK2322]
const stringReturn = (() => { return PublicationString.A; }) satisfies () => PublicationString.A;
let stringReturnCopy = stringReturn();
stringReturnCopy = PublicationString.B; // error[TK2322]
