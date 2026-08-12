declare enum AmbientNumeric {
  First,
  Fixed = 4,
  Later,
}

declare const ambient: AmbientNumeric;
const ambientWhole: AmbientNumeric = ambient;
const ambientMember: AmbientNumeric.Fixed = AmbientNumeric.Fixed;
const ambientNumber: number = AmbientNumeric.Later;
const ambientArbitrary: AmbientNumeric.First = 99;

declare enum AmbientComputed {
  Bad = nextAmbient(), // error[TK1066]
}

declare function nextAmbient(): number;

const enum ConstCorpus {
  Classic = "classic",
  Tech = "tech",
}

const enum ConstNonFinite {
  Infinity = 1 / 0, // error[TK2477]
  NotANumber = 0 / 0, // error[TK2478]
}

const constWhole: ConstCorpus = ConstCorpus.Classic;
const constMember: ConstCorpus.Tech = ConstCorpus.Tech;
const constLiteral: "tech" = ConstCorpus.Tech;
const constRaw: ConstCorpus = "classic"; // error[TK2322]
