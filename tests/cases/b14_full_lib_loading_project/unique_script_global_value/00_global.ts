// Unique script `var` contributes to the effective global object even without a lib-name collision.
declare var B14UniqueGlobal: {
  count: number;
};

function B14GlobalFunction(): number {
  return 1;
}

let B14GlobalLet = 1;
const B14GlobalConst = 1;
class B14GlobalClass {
  value: number = 1;
}
