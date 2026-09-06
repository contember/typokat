(satisfiesWrite satisfies unknown) = rhs;
(satisfiesUpdate satisfies unknown)++;
(satisfiesCompound satisfies unknown) += rhs;
({ value: (satisfiesDestructure satisfies unknown) } = source);
for ((satisfiesLoop satisfies unknown) of values) {}

(asWrite as unknown) = rhs;
(<unknown>angleWrite) = rhs;
nonNullWrite! = rhs;
(receiver.member satisfies unknown) = rhs;
(computedReceiver[computedKey] satisfies unknown) += rhs;
const notACopy = (copySource satisfies unknown);
