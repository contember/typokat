// JS Number::toString switches to exponential notation at 1e21 and below 1e-6.
// Cross-checked against tsc 6.0.3 --strict.

type FixedUpperBoundary = `${1e20}`;
const fixedUpper: FixedUpperBoundary = "100000000000000000000";
const fixedUpperWrong: FixedUpperBoundary = "1e+20"; // error[TK2322]

type ExponentialUpperBoundary = `${1e21}`;
const exponentialUpper: ExponentialUpperBoundary = "1e+21";
const exponentialUpperWrong: ExponentialUpperBoundary = "1000000000000000000000"; // error[TK2322]

type FixedLowerBoundary = `${1e-6}`;
const fixedLower: FixedLowerBoundary = "0.000001";
const fixedLowerWrong: FixedLowerBoundary = "1e-6"; // error[TK2322]

type ExponentialLowerBoundary = `${1e-7}`;
const exponentialLower: ExponentialLowerBoundary = "1e-7";
const exponentialLowerWrong: ExponentialLowerBoundary = "0.0000001"; // error[TK2322]

type NegativeLarge = `${-1e21}`;
const negativeLarge: NegativeLarge = "-1e+21";
const negativeLargeWrong: NegativeLarge = "-1000000000000000000000"; // error[TK2322]

type NegativeSmall = `${-1e-7}`;
const negativeSmall: NegativeSmall = "-1e-7";
const negativeSmallWrong: NegativeSmall = "-0.0000001"; // error[TK2322]

type NegativeZero = `${-0}`;
const negativeZero: NegativeZero = "0";
const negativeZeroWrong: NegativeZero = "-0"; // error[TK2322]

type MaximumFinite = `${1.7976931348623157e308}`;
const maximumFinite: MaximumFinite = "1.7976931348623157e+308";
const maximumFiniteWrong: MaximumFinite = "1.7976931348623157e308"; // error[TK2322]

type MinimumSubnormal = `${5e-324}`;
const minimumSubnormal: MinimumSubnormal = "5e-324";
const minimumSubnormalWrong: MinimumSubnormal = "4.9406564584124654e-324"; // error[TK2322]

type ShortestRoundTrip = `${1.2345678901234567}`;
const shortestRoundTrip: ShortestRoundTrip = "1.2345678901234567";
const shortestRoundTripWrong: ShortestRoundTrip = "1.2345678901234566"; // error[TK2322]
