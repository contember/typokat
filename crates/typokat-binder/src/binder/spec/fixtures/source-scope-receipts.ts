declare function rootProducer<T>(): T;
const rootValue = rootProducer();

function outer(value = (() => rootValue)()) {
  const local = value;
  {
    const local = rootValue;
    var hoisted = (() => local)();
    function nested() { return local; }
  }
  for (let index = (() => local)(); (() => index)(); (() => index)()) {
    const bodyValue = index;
  }
  for (const item of (() => [local])()) {
    const bodyValue = item;
  }
  for (const property in (() => ({ local }))()) {
    const bodyValue = property;
  }
  switch ((() => local)()) {
    case (() => local)(): {
      const caseValue = local;
      break;
    }
    default: break;
  }
  try { consume(local); }
  catch (caught) { const caughtValue = (() => caught)(); }
  finally { consume(local); }
  return local;
}

const expression = function named(value = (() => rootValue)()) { return value; };
const arrow = (value = (() => rootValue)()) => value;
