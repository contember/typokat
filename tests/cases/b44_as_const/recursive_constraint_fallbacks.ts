// Constraint fallback must not substitute its previous expansion back into itself.

declare function recursiveFactory<T extends { next: T }>(): T;
const recursiveFactoryResult = recursiveFactory();
declare function mutualFactory<U extends { value: T }, T extends { value: U }>(): [U, T];
const mutualFactoryResult = mutualFactory();

interface RecursiveCandidate {
  next: RecursiveCandidate;
}
declare const recursiveCandidate: RecursiveCandidate;
declare function checkedRecursiveCandidate<T extends { next: T }>(value: T): T;
const acceptedRecursiveCandidate = checkedRecursiveCandidate(recursiveCandidate);
checkedRecursiveCandidate(1); // error[TK2345]
