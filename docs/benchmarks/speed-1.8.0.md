# Speed benchmark results

Click a column header to sort. Use the filter to narrow large result sets.

<details>
<summary>Provenance fingerprint</summary>

```json
{
  "arch": "x86_64",
  "cpu": "Intel(R) Core(TM) i7-6700HQ CPU @ 2.60GHz",
  "cpu_count": 8,
  "git_sha": "28ffe7bf1bbe1127b1d2fcefe59f4fc2c9fbbd6f",
  "hostname": "unknown",
  "hyperfine": "hyperfine 2.0.0",
  "kernel": "Linux 7.1.3-arch1-2",
  "load1": "1.07",
  "os": "linux",
  "rustc": "rustc 1.97.0 (2d8144b78 2026-07-07)"
}
```

</details>

| benchmark | time |
|---|---:|
| competitor_values/adotenv/plain/8 | 948 ns/iter (+/- 1) |
| competitor_values/adotenv-compact/plain/8 | 900 ns/iter (+/- 0) |
| competitor_values/adotenv-compact-entries/plain/8 | 897 ns/iter (+/- 1) |
| competitor_values/dotenv-parser/plain/8 | 13,842 ns/iter (+/- 19) |
| competitor_values/dotenvs/plain/8 | 3,257 ns/iter (+/- 4) |
| competitor_values/dotenvy/plain/8 | 4,598 ns/iter (+/- 5) |
| competitor_values/adotenv/long-line/8 | 1,685 ns/iter (+/- 1) |
| competitor_values/adotenv-compact/long-line/8 | 1,447 ns/iter (+/- 0) |
| competitor_values/adotenv-compact-entries/long-line/8 | 1,432 ns/iter (+/- 0) |
| competitor_values/dotenv-parser/long-line/8 | 57,406 ns/iter (+/- 46) |
| competitor_values/dotenvs/long-line/8 | 4,734 ns/iter (+/- 2) |
| competitor_values/dotenvy/long-line/8 | 9,607 ns/iter (+/- 21) |
| competitor_values/adotenv/plain/128 | 22,113 ns/iter (+/- 17) |
| competitor_values/adotenv-compact/plain/128 | 12,276 ns/iter (+/- 7) |
| competitor_values/adotenv-compact-entries/plain/128 | 12,163 ns/iter (+/- 6) |
| competitor_values/dotenv-parser/plain/128 | 227,149 ns/iter (+/- 434) |
| competitor_values/dotenvs/plain/128 | 59,089 ns/iter (+/- 54) |
| competitor_values/dotenvy/plain/128 | 83,582 ns/iter (+/- 127) |
| competitor_values/adotenv/long-line/128 | 29,683 ns/iter (+/- 42) |
| competitor_values/adotenv-compact/long-line/128 | 20,222 ns/iter (+/- 5) |
| competitor_values/adotenv-compact-entries/long-line/128 | 20,042 ns/iter (+/- 19) |
| competitor_values/dotenv-parser/long-line/128 | 895,386 ns/iter (+/- 716) |
| competitor_values/dotenvs/long-line/128 | 90,234 ns/iter (+/- 56) |
| competitor_values/dotenvy/long-line/128 | 168,996 ns/iter (+/- 108) |
| competitor_values/adotenv/plain/1k | 169,710 ns/iter (+/- 83) |
| competitor_values/adotenv-compact/plain/1k | 91,906 ns/iter (+/- 66) |
| competitor_values/adotenv-compact-entries/plain/1k | 90,403 ns/iter (+/- 32) |
| competitor_values/dotenv-parser/plain/1k | 1,881,070 ns/iter (+/- 3,475) |
| competitor_values/dotenvs/plain/1k | 444,935 ns/iter (+/- 1,089) |
| competitor_values/dotenvy/plain/1k | 647,454 ns/iter (+/- 5,125) |
| competitor_values/adotenv/long-line/1k | 232,217 ns/iter (+/- 209) |
| competitor_values/adotenv-compact/long-line/1k | 152,950 ns/iter (+/- 74) |
| competitor_values/adotenv-compact-entries/long-line/1k | 151,505 ns/iter (+/- 84) |
| competitor_values/dotenv-parser/long-line/1k | 7,057,553 ns/iter (+/- 5,830) |
| competitor_values/dotenvs/long-line/1k | 691,575 ns/iter (+/- 633) |
| competitor_values/dotenvy/long-line/1k | 1,307,942 ns/iter (+/- 1,291) |
| competitor_values/adotenv/plain/10k | 1,736,627 ns/iter (+/- 1,382) |
| competitor_values/adotenv-compact/plain/10k | 937,612 ns/iter (+/- 416) |
| competitor_values/adotenv-compact-entries/plain/10k | 923,650 ns/iter (+/- 500) |
| competitor_values/dotenv-parser/plain/10k | 20,390,289 ns/iter (+/- 45,962) |
| competitor_values/dotenvs/plain/10k | 4,463,391 ns/iter (+/- 10,020) |
| competitor_values/dotenvy/plain/10k | 6,483,157 ns/iter (+/- 20,313) |
| competitor_values/adotenv/long-line/10k | 2,327,399 ns/iter (+/- 1,633) |
| competitor_values/adotenv-compact/long-line/10k | 1,538,085 ns/iter (+/- 1,581) |
| competitor_values/adotenv-compact-entries/long-line/10k | 1,523,190 ns/iter (+/- 1,261) |
| competitor_values/dotenv-parser/long-line/10k | 71,451,954 ns/iter (+/- 236,664) |
| competitor_values/dotenvs/long-line/10k | 7,583,190 ns/iter (+/- 27,333) |
| competitor_values/dotenvy/long-line/10k | 13,456,929 ns/iter (+/- 48,139) |
| competitor_values/adotenv/plain/100k | 19,063,466 ns/iter (+/- 75,047) |
| competitor_values/adotenv-compact/plain/100k | 10,046,291 ns/iter (+/- 15,743) |
| competitor_values/adotenv-compact-entries/plain/100k | 9,924,351 ns/iter (+/- 48,457) |
| competitor_values/dotenv-parser/plain/100k | 226,115,686 ns/iter (+/- 503,840) |
| competitor_values/dotenvs/plain/100k | 63,232,482 ns/iter (+/- 189,877) |
| competitor_values/dotenvy/plain/100k | 97,919,588 ns/iter (+/- 205,131) |
| competitor_values/adotenv/long-line/100k | 25,456,607 ns/iter (+/- 41,849) |
| competitor_values/adotenv-compact/long-line/100k | 15,690,543 ns/iter (+/- 15,790) |
| competitor_values/adotenv-compact-entries/long-line/100k | 15,534,459 ns/iter (+/- 15,663) |
| competitor_values/dotenv-parser/long-line/100k | 726,016,564 ns/iter (+/- 1,784,365) |
| competitor_values/dotenvs/long-line/100k | 94,777,344 ns/iter (+/- 432,281) |
| competitor_values/dotenvy/long-line/100k | 166,945,744 ns/iter (+/- 256,969) |
| competitor_load/adotenv/plain/8 | 5,725 ns/iter (+/- 123) |
| competitor_load/adotenv-compact/plain/8 | 5,472 ns/iter (+/- 123) |
| competitor_load/dotenv/plain/8 | 6,549 ns/iter (+/- 79) |
| competitor_load/dotenv-flow/plain/8 | 6,502 ns/iter (+/- 57) |
| competitor_load/dotenvs/plain/8 | 6,595 ns/iter (+/- 106) |
| competitor_load/dotenvy/plain/8 | 7,068 ns/iter (+/- 78) |
| competitor_load/adotenv/long-line/8 | 6,308 ns/iter (+/- 128) |
| competitor_load/adotenv-compact/long-line/8 | 6,103 ns/iter (+/- 118) |
| competitor_load/dotenv/long-line/8 | 10,485 ns/iter (+/- 74) |
| competitor_load/dotenv-flow/long-line/8 | 10,030 ns/iter (+/- 64) |
| competitor_load/dotenvs/long-line/8 | 8,139 ns/iter (+/- 57) |
| competitor_load/dotenvy/long-line/8 | 11,847 ns/iter (+/- 79) |
| competitor_load/adotenv/empty/8 | 3,792 ns/iter (+/- 3) |
| competitor_load/adotenv-compact/empty/8 | 3,800 ns/iter (+/- 62) |
| competitor_load/dotenv/empty/8 | 1,818 ns/iter (+/- 7) |
| competitor_load/dotenv-flow/empty/8 | 1,818 ns/iter (+/- 27) |
| competitor_load/dotenvs/empty/8 | 2,655 ns/iter (+/- 36) |
| competitor_load/dotenvy/empty/8 | 1,814 ns/iter (+/- 24) |
| competitor_load/adotenv/plain/128 | 27,472 ns/iter (+/- 111) |
| competitor_load/adotenv-compact/plain/128 | 17,111 ns/iter (+/- 91) |
| competitor_load/dotenv/plain/128 | 78,128 ns/iter (+/- 138) |
| competitor_load/dotenv-flow/plain/128 | 78,008 ns/iter (+/- 127) |
| competitor_load/dotenvs/plain/128 | 62,828 ns/iter (+/- 81) |
| competitor_load/dotenvy/plain/128 | 86,171 ns/iter (+/- 111) |
| competitor_load/adotenv/long-line/128 | 35,712 ns/iter (+/- 130) |
| competitor_load/adotenv-compact/long-line/128 | 26,088 ns/iter (+/- 131) |
| competitor_load/dotenv/long-line/128 | 148,671 ns/iter (+/- 126) |
| competitor_load/dotenv-flow/long-line/128 | 147,018 ns/iter (+/- 150) |
| competitor_load/dotenvs/long-line/128 | 94,804 ns/iter (+/- 149) |
| competitor_load/dotenvy/long-line/128 | 173,285 ns/iter (+/- 539) |
| competitor_load/adotenv/empty/128 | 3,820 ns/iter (+/- 89) |
| competitor_load/adotenv-compact/empty/128 | 3,981 ns/iter (+/- 95) |
| competitor_load/dotenv/empty/128 | 1,831 ns/iter (+/- 54) |
| competitor_load/dotenv-flow/empty/128 | 1,815 ns/iter (+/- 50) |
| competitor_load/dotenvs/empty/128 | 2,648 ns/iter (+/- 17) |
| competitor_load/dotenvy/empty/128 | 1,841 ns/iter (+/- 59) |
| competitor_load/adotenv/plain/1k | 180,207 ns/iter (+/- 230) |
| competitor_load/adotenv-compact/plain/1k | 99,030 ns/iter (+/- 163) |
| competitor_load/dotenv/plain/1k | 586,958 ns/iter (+/- 3,043) |
| competitor_load/dotenv-flow/plain/1k | 586,744 ns/iter (+/- 1,019) |
| competitor_load/dotenvs/plain/1k | 452,136 ns/iter (+/- 285) |
| competitor_load/dotenvy/plain/1k | 652,381 ns/iter (+/- 1,130) |
| competitor_load/adotenv/long-line/1k | 248,825 ns/iter (+/- 291) |
| competitor_load/adotenv-compact/long-line/1k | 168,434 ns/iter (+/- 88) |
| competitor_load/dotenv/long-line/1k | 1,142,751 ns/iter (+/- 5,577) |
| competitor_load/dotenv-flow/long-line/1k | 1,113,303 ns/iter (+/- 2,777) |
| competitor_load/dotenvs/long-line/1k | 699,934 ns/iter (+/- 387) |
| competitor_load/dotenvy/long-line/1k | 1,313,475 ns/iter (+/- 2,415) |
| competitor_load/adotenv/empty/1k | 3,795 ns/iter (+/- 89) |
| competitor_load/adotenv-compact/empty/1k | 3,796 ns/iter (+/- 12) |
| competitor_load/dotenv/empty/1k | 1,819 ns/iter (+/- 35) |
| competitor_load/dotenv-flow/empty/1k | 1,816 ns/iter (+/- 21) |
| competitor_load/dotenvs/empty/1k | 2,655 ns/iter (+/- 7) |
| competitor_load/dotenvy/empty/1k | 1,816 ns/iter (+/- 23) |
| competitor_load/adotenv/plain/10k | 1,767,713 ns/iter (+/- 1,999) |
| competitor_load/adotenv-compact/plain/10k | 966,063 ns/iter (+/- 589) |
| competitor_load/dotenv/plain/10k | 5,744,768 ns/iter (+/- 21,535) |
| competitor_load/dotenv-flow/plain/10k | 5,737,974 ns/iter (+/- 22,120) |
| competitor_load/dotenvs/plain/10k | 4,468,373 ns/iter (+/- 8,054) |
| competitor_load/dotenvy/plain/10k | 6,494,864 ns/iter (+/- 29,378) |
| competitor_load/adotenv/long-line/10k | 2,439,583 ns/iter (+/- 4,578) |
| competitor_load/adotenv-compact/long-line/10k | 1,635,669 ns/iter (+/- 1,227) |
| competitor_load/dotenv/long-line/10k | 11,405,200 ns/iter (+/- 59,312) |
| competitor_load/dotenv-flow/long-line/10k | 11,281,441 ns/iter (+/- 58,456) |
| competitor_load/dotenvs/long-line/10k | 7,558,013 ns/iter (+/- 17,919) |
| competitor_load/dotenvy/long-line/10k | 13,319,679 ns/iter (+/- 61,021) |
| competitor_load/adotenv/empty/10k | 3,796 ns/iter (+/- 49) |
| competitor_load/adotenv-compact/empty/10k | 3,802 ns/iter (+/- 76) |
| competitor_load/dotenv/empty/10k | 1,821 ns/iter (+/- 12) |
| competitor_load/dotenv-flow/empty/10k | 1,823 ns/iter (+/- 32) |
| competitor_load/dotenvs/empty/10k | 2,668 ns/iter (+/- 36) |
| competitor_load/dotenvy/empty/10k | 1,820 ns/iter (+/- 14) |
| competitor_load/adotenv/plain/100k | 19,650,487 ns/iter (+/- 24,183) |
| competitor_load/adotenv-compact/plain/100k | 10,433,555 ns/iter (+/- 13,424) |
| competitor_load/dotenv/plain/100k | 87,047,111 ns/iter (+/- 1,574,126) |
| competitor_load/dotenv-flow/plain/100k | 90,965,525 ns/iter (+/- 1,806,556) |
| competitor_load/dotenvs/plain/100k | 63,204,289 ns/iter (+/- 200,277) |
| competitor_load/dotenvy/plain/100k | 97,630,321 ns/iter (+/- 1,835,243) |
| competitor_load/adotenv/long-line/100k | 27,597,683 ns/iter (+/- 28,903) |
| competitor_load/adotenv-compact/long-line/100k | 17,778,069 ns/iter (+/- 23,666) |
| competitor_load/dotenv/long-line/100k | 150,241,711 ns/iter (+/- 268,312) |
| competitor_load/dotenv-flow/long-line/100k | 147,806,125 ns/iter (+/- 225,138) |
| competitor_load/dotenvs/long-line/100k | 95,552,351 ns/iter (+/- 215,385) |
| competitor_load/dotenvy/long-line/100k | 168,331,392 ns/iter (+/- 407,667) |
| competitor_load/adotenv/empty/100k | 3,800 ns/iter (+/- 93) |
| competitor_load/adotenv-compact/empty/100k | 3,802 ns/iter (+/- 19) |
| competitor_load/dotenv/empty/100k | 1,825 ns/iter (+/- 26) |
| competitor_load/dotenv-flow/empty/100k | 1,828 ns/iter (+/- 26) |
| competitor_load/dotenvs/empty/100k | 2,739 ns/iter (+/- 24) |
| competitor_load/dotenvy/empty/100k | 1,884 ns/iter (+/- 69) |
