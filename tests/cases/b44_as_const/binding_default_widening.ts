// Inferred binding defaults contribute their own type instead of being checked against
// only the projected source literal.

let { value: ordinaryDefault = "foo" } = { value: "bar" as "bar" };
ordinaryDefault = "baz";

let { value: exactDefault = "foo" as "foo" } = { value: "bar" as "bar" };
exactDefault = "foo";
exactDefault = "bar";
exactDefault = "baz"; // error[TK2322]

let { value: constDefault = "foo" as const } = { value: "bar" as const };
constDefault = "foo";
constDefault = "bar";
constDefault = "baz"; // error[TK2322]
