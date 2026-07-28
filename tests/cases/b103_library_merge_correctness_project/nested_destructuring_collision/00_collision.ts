declare const condition: boolean;
declare const source: {
  nested: [{ ctor: typeof RegExp }];
};

if (condition) {
  var {
    nested: [{ ctor: RegExp }],
  } = source;
}
