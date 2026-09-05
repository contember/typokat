// Primitive constraints apply their literal policy to the final merged candidate.

declare function constrainedStrings<T extends string>(left: T, right: T): T;
let distinctConstrainedStrings = constrainedStrings("a", "b");
distinctConstrainedStrings = "c"; // error[TK2322]
let equalConstrainedStrings = constrainedStrings("a", "a");
equalConstrainedStrings = "c"; // error[TK2322]
let firstExactConstrainedString = constrainedStrings("a" as const, "b");
firstExactConstrainedString = "c"; // error[TK2322]
let lastExactConstrainedString = constrainedStrings("a", "b" as const);
lastExactConstrainedString = "c"; // error[TK2322]

declare function constrainedNumbers<T extends number>(left: T, right: T): T;
let distinctConstrainedNumbers = constrainedNumbers(1, 2);
distinctConstrainedNumbers = 3; // error[TK2322]
let equalConstrainedNumbers = constrainedNumbers(1, 1);
equalConstrainedNumbers = 3; // error[TK2322]

declare function constrainedObject<T extends string>(left: T, right: T): { value: T };
const constrainedObjectResult = constrainedObject("a", "b");
constrainedObjectResult.value = "c"; // error[TK2322]

declare function unconstrainedPair<T>(left: T, right: T): T;
let ordinaryPairResult = unconstrainedPair("a", "b");
ordinaryPairResult = "c";
