// Only complete published member and element surfaces may enter class const initializers.

declare class PublishedReferenceSource {
  value: number;
}
declare const publishedReferenceSource: PublishedReferenceSource;
declare const tupleReferenceSource: [number];

class PublishedReferenceFields {
  object = ({ value: publishedReferenceSource.value }) as const;
  tuple = ({ value: tupleReferenceSource[0] }) as const;
}

declare const publishedReferenceFields: PublishedReferenceFields;
const publishedObjectValue: number = publishedReferenceFields.object.value;
const publishedTupleValue: number = publishedReferenceFields.tuple.value;
publishedReferenceFields.object.value = 1; // error[TK2540]
publishedReferenceFields.tuple.value = 1; // error[TK2540]
