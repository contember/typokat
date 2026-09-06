declare const publicationFlag: boolean;

const groupedPublication = [publicationFlag ? true : 1] satisfies (boolean | number)[];
const groupedPublicationExact: true | 1 = groupedPublication[0];
let groupedPublicationCopy = groupedPublication[0];
groupedPublicationCopy = 2; // error[TK2322]
groupedPublicationCopy = false; // error[TK2322]
groupedPublicationCopy = true;
groupedPublicationCopy = 1;

const separatePublication = [true, 1] satisfies (boolean | number)[];
let separatePublicationCopy = separatePublication[0];
separatePublicationCopy = 2;
separatePublicationCopy = false; // error[TK2322]

const groupedPublicationArrow = (() => publicationFlag ? true : 1) satisfies () => boolean | number;
const groupedPublicationArrowExact: true | 1 = groupedPublicationArrow();
let groupedPublicationArrowCopy = groupedPublicationArrow();
groupedPublicationArrowCopy = 2;
groupedPublicationArrowCopy = false;

const groupedPublicationObject = { a: publicationFlag ? "a" : 1 } satisfies { a: "a" | number };
let groupedPublicationObjectCopy = groupedPublicationObject.a;
groupedPublicationObjectCopy = 2; // error[TK2322]
groupedPublicationObjectCopy = "b"; // error[TK2322]

const groupedPublicationTuple = [publicationFlag ? true : 1] satisfies [number | boolean];
let groupedPublicationTupleCopy = groupedPublicationTuple[0];
groupedPublicationTupleCopy = 2; // error[TK2322]
groupedPublicationTupleCopy = false; // error[TK2322]
