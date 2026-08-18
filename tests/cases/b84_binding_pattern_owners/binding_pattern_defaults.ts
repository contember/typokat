const { variable = (() => { throw 1; })() } = { variable: 1 };

function readParameter(
  { parameter = (() => { throw 2; })() }: { parameter?: number },
): number {
  return parameter;
}

readParameter({});

try {
  throw {};
} catch ({ caught = (() => { throw 3; })() }: any) {
  caught;
}
