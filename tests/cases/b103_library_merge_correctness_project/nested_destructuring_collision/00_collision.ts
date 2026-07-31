declare const condition: boolean;
declare const source: {
  nested: [{ ctor: RegExpConstructor }];
};

if (condition) {
  var {
    nested: [{ ctor: RegExp }],
  } = source;
}
