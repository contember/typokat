// A static computed write observes readonly object-property metadata as well as readonly
// collection wrappers.

const computedFrozenObject = ({ value: 1 }) as const;

computedFrozenObject["value"] = 1; // error[TK2540]: Cannot assign to 'value' because it is a read-only property
