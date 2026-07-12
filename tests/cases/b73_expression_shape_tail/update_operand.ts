// tsc 6.0.3 --strict: TS2304 on Missing; the numeric for-loop is clean.

Missing++; // error[TK2304]: Cannot find name 'Missing'

for (let i = 0; i < 3; i++) {}
