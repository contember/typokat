const { variable = (() => { throw 1; })() } = {} as any;

function readParameter(
  { parameter = (() => { throw 2; })() }: { parameter?: number }, // incomplete[bind/binding-pattern/object-pattern]
): number {
  return parameter;
}

readParameter({});

try {
  throw {};
} catch ({ caught = (() => { throw 3; })() }: any) { // incomplete[stmt-check/try-statement/catch-param]
  caught;
}
