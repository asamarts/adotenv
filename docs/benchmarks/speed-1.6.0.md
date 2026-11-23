# Speed benchmark results

Click a column header to sort. Use the filter to narrow large result sets.

<details>
<summary>Provenance fingerprint</summary>

```json
{
  "arch": "x86_64",
  "cpu": "Intel(R) Core(TM) i7-6700HQ CPU @ 2.60GHz",
  "cpu_count": 8,
  "git_sha": "79860ac9b2d6c8b7a37772d692c6f82a9d8a1ac9",
  "hostname": "unknown",
  "hyperfine": "hyperfine 2.0.0",
  "kernel": "Linux 7.1.3-arch1-2",
  "load1": "0.91",
  "os": "linux",
  "rustc": "rustc 1.97.1 (8bab26f4f 2026-07-14)"
}
```

</details>

| benchmark | time |
|---|---:|
| competitor_values/adotenv/plain/8 | 961 ns/iter (+/- 3) |
| competitor_values/adotenv-compact/plain/8 | 899 ns/iter (+/- 2) |
| competitor_values/adotenv-compact-entries/plain/8 | 854 ns/iter (+/- 1) |
| competitor_values/dotenv-parser/plain/8 | 14,126 ns/iter (+/- 12) |
| competitor_values/dotenvs/plain/8 | 3,300 ns/iter (+/- 5) |
| competitor_values/dotenvy/plain/8 | 4,594 ns/iter (+/- 6) |
| competitor_values/adotenv/long-line/8 | 1,685 ns/iter (+/- 2) |
| competitor_values/adotenv-compact/long-line/8 | 1,424 ns/iter (+/- 0) |
| competitor_values/adotenv-compact-entries/long-line/8 | 1,402 ns/iter (+/- 0) |
| competitor_values/dotenv-parser/long-line/8 | 57,905 ns/iter (+/- 34) |
| competitor_values/dotenvs/long-line/8 | 4,758 ns/iter (+/- 2) |
| competitor_values/dotenvy/long-line/8 | 10,282 ns/iter (+/- 13) |
| competitor_values/adotenv/plain/128 | 22,070 ns/iter (+/- 37) |
| competitor_values/adotenv-compact/plain/128 | 11,978 ns/iter (+/- 4) |
| competitor_values/adotenv-compact-entries/plain/128 | 11,853 ns/iter (+/- 8) |
| competitor_values/dotenv-parser/plain/128 | 226,954 ns/iter (+/- 154) |
| competitor_values/dotenvs/plain/128 | 59,304 ns/iter (+/- 94) |
| competitor_values/dotenvy/plain/128 | 83,624 ns/iter (+/- 95) |
| competitor_values/adotenv/long-line/128 | 29,718 ns/iter (+/- 14) |
| competitor_values/adotenv-compact/long-line/128 | 20,230 ns/iter (+/- 9) |
| competitor_values/adotenv-compact-entries/long-line/128 | 19,927 ns/iter (+/- 11) |
| competitor_values/dotenv-parser/long-line/128 | 910,713 ns/iter (+/- 340) |
| competitor_values/dotenvs/long-line/128 | 89,653 ns/iter (+/- 109) |
| competitor_values/dotenvy/long-line/128 | 175,977 ns/iter (+/- 1,679) |
| competitor_values/adotenv/plain/1k | 172,434 ns/iter (+/- 179) |
| competitor_values/adotenv-compact/plain/1k | 89,186 ns/iter (+/- 74) |
| competitor_values/adotenv-compact-entries/plain/1k | 87,791 ns/iter (+/- 37) |
| competitor_values/dotenv-parser/plain/1k | 1,883,540 ns/iter (+/- 1,835) |
| competitor_values/dotenvs/plain/1k | 448,412 ns/iter (+/- 663) |
| competitor_values/dotenvy/plain/1k | 655,366 ns/iter (+/- 791) |
| competitor_values/adotenv/long-line/1k | 232,485 ns/iter (+/- 204) |
| competitor_values/adotenv-compact/long-line/1k | 152,000 ns/iter (+/- 78) |
| competitor_values/adotenv-compact-entries/long-line/1k | 150,630 ns/iter (+/- 61) |
| competitor_values/dotenv-parser/long-line/1k | 7,163,413 ns/iter (+/- 2,531) |
| competitor_values/dotenvs/long-line/1k | 689,573 ns/iter (+/- 2,198) |
| competitor_values/dotenvy/long-line/1k | 1,356,334 ns/iter (+/- 1,388) |
| competitor_values/adotenv/plain/10k | 1,736,824 ns/iter (+/- 1,963) |
| competitor_values/adotenv-compact/plain/10k | 908,743 ns/iter (+/- 4,952) |
| competitor_values/adotenv-compact-entries/plain/10k | 894,929 ns/iter (+/- 1,268) |
| competitor_values/dotenv-parser/plain/10k | 20,459,828 ns/iter (+/- 91,120) |
| competitor_values/dotenvs/plain/10k | 4,565,673 ns/iter (+/- 20,498) |
| competitor_values/dotenvy/plain/10k | 6,524,369 ns/iter (+/- 41,046) |
| competitor_values/adotenv/long-line/10k | 2,313,107 ns/iter (+/- 9,613) |
| competitor_values/adotenv-compact/long-line/10k | 1,502,311 ns/iter (+/- 3,970) |
| competitor_values/adotenv-compact-entries/long-line/10k | 1,487,846 ns/iter (+/- 23,937) |
| competitor_values/dotenv-parser/long-line/10k | 72,288,792 ns/iter (+/- 325,816) |
| competitor_values/dotenvs/long-line/10k | 7,461,679 ns/iter (+/- 62,023) |
| competitor_values/dotenvy/long-line/10k | 13,727,160 ns/iter (+/- 53,727) |
| competitor_values/adotenv/plain/100k | 19,008,518 ns/iter (+/- 104,471) |
| competitor_values/adotenv-compact/plain/100k | 9,881,302 ns/iter (+/- 9,175) |
| competitor_values/adotenv-compact-entries/plain/100k | 9,769,580 ns/iter (+/- 8,605) |
| competitor_values/dotenv-parser/plain/100k | 226,472,316 ns/iter (+/- 187,824) |
| competitor_values/dotenvs/plain/100k | 63,213,472 ns/iter (+/- 2,179,330) |
| competitor_values/dotenvy/plain/100k | 94,608,226 ns/iter (+/- 700,687) |
| competitor_values/adotenv/long-line/100k | 25,355,603 ns/iter (+/- 51,985) |
| competitor_values/adotenv-compact/long-line/100k | 15,491,171 ns/iter (+/- 11,746) |
| competitor_values/adotenv-compact-entries/long-line/100k | 15,343,055 ns/iter (+/- 10,541) |
| competitor_values/dotenv-parser/long-line/100k | 737,587,258 ns/iter (+/- 843,035) |
| competitor_values/dotenvs/long-line/100k | 94,534,308 ns/iter (+/- 1,356,019) |
| competitor_values/dotenvy/long-line/100k | 172,410,545 ns/iter (+/- 616,957) |
| competitor_load/adotenv/plain/8 | 5,725 ns/iter (+/- 75) |
| competitor_load/adotenv-compact/plain/8 | 5,566 ns/iter (+/- 95) |
| competitor_load/dotenv/plain/8 | 7,093 ns/iter (+/- 18) |
| competitor_load/dotenv-flow/plain/8 | 7,225 ns/iter (+/- 60) |
| competitor_load/dotenvs/plain/8 | 7,179 ns/iter (+/- 108) |
| competitor_load/dotenvy/plain/8 | 7,589 ns/iter (+/- 46) |
| competitor_load/adotenv/long-line/8 | 6,394 ns/iter (+/- 67) |
| competitor_load/adotenv-compact/long-line/8 | 6,276 ns/iter (+/- 46) |
| competitor_load/dotenv/long-line/8 | 11,055 ns/iter (+/- 65) |
| competitor_load/dotenv-flow/long-line/8 | 11,362 ns/iter (+/- 70) |
| competitor_load/dotenvs/long-line/8 | 8,816 ns/iter (+/- 88) |
| competitor_load/dotenvy/long-line/8 | 13,335 ns/iter (+/- 76) |
| competitor_load/adotenv/empty/8 | 799 ns/iter (+/- 2) |
| competitor_load/adotenv-compact/empty/8 | 813 ns/iter (+/- 21) |
| competitor_load/dotenv/empty/8 | 2,421 ns/iter (+/- 46) |
| competitor_load/dotenv-flow/empty/8 | 2,426 ns/iter (+/- 55) |
| competitor_load/dotenvs/empty/8 | 3,250 ns/iter (+/- 74) |
| competitor_load/dotenvy/empty/8 | 2,431 ns/iter (+/- 44) |
| competitor_load/adotenv/plain/128 | 28,169 ns/iter (+/- 75) |
| competitor_load/adotenv-compact/plain/128 | 17,147 ns/iter (+/- 102) |
| competitor_load/dotenv/plain/128 | 78,684 ns/iter (+/- 96) |
| competitor_load/dotenv-flow/plain/128 | 79,467 ns/iter (+/- 85) |
| competitor_load/dotenvs/plain/128 | 64,674 ns/iter (+/- 161) |
| competitor_load/dotenvy/plain/128 | 87,964 ns/iter (+/- 106) |
| competitor_load/adotenv/long-line/128 | 36,884 ns/iter (+/- 134) |
| competitor_load/adotenv-compact/long-line/128 | 26,404 ns/iter (+/- 97) |
| competitor_load/dotenv/long-line/128 | 144,983 ns/iter (+/- 307) |
| competitor_load/dotenv-flow/long-line/128 | 150,450 ns/iter (+/- 170) |
| competitor_load/dotenvs/long-line/128 | 95,374 ns/iter (+/- 119) |
| competitor_load/dotenvy/long-line/128 | 181,478 ns/iter (+/- 167) |
| competitor_load/adotenv/empty/128 | 811 ns/iter (+/- 19) |
| competitor_load/adotenv-compact/empty/128 | 831 ns/iter (+/- 19) |
| competitor_load/dotenv/empty/128 | 2,510 ns/iter (+/- 28) |
| competitor_load/dotenv-flow/empty/128 | 2,510 ns/iter (+/- 48) |
| competitor_load/dotenvs/empty/128 | 3,234 ns/iter (+/- 58) |
| competitor_load/dotenvy/empty/128 | 2,553 ns/iter (+/- 57) |
| competitor_load/adotenv/plain/1k | 181,607 ns/iter (+/- 166) |
| competitor_load/adotenv-compact/plain/1k | 97,218 ns/iter (+/- 140) |
| competitor_load/dotenv/plain/1k | 583,749 ns/iter (+/- 1,327) |
| competitor_load/dotenv-flow/plain/1k | 594,350 ns/iter (+/- 3,102) |
| competitor_load/dotenvs/plain/1k | 454,048 ns/iter (+/- 1,605) |
| competitor_load/dotenvy/plain/1k | 657,597 ns/iter (+/- 6,286) |
| competitor_load/adotenv/long-line/1k | 248,288 ns/iter (+/- 208) |
| competitor_load/adotenv-compact/long-line/1k | 167,956 ns/iter (+/- 152) |
| competitor_load/dotenv/long-line/1k | 1,094,004 ns/iter (+/- 2,856) |
| competitor_load/dotenv-flow/long-line/1k | 1,124,498 ns/iter (+/- 4,151) |
| competitor_load/dotenvs/long-line/1k | 701,365 ns/iter (+/- 541) |
| competitor_load/dotenvy/long-line/1k | 1,364,797 ns/iter (+/- 3,507) |
| competitor_load/adotenv/empty/1k | 798 ns/iter (+/- 12) |
| competitor_load/adotenv-compact/empty/1k | 807 ns/iter (+/- 2) |
| competitor_load/dotenv/empty/1k | 2,421 ns/iter (+/- 37) |
| competitor_load/dotenv-flow/empty/1k | 2,425 ns/iter (+/- 36) |
| competitor_load/dotenvs/empty/1k | 3,245 ns/iter (+/- 53) |
| competitor_load/dotenvy/empty/1k | 2,535 ns/iter (+/- 58) |
| competitor_load/adotenv/plain/10k | 1,770,441 ns/iter (+/- 28,382) |
| competitor_load/adotenv-compact/plain/10k | 930,683 ns/iter (+/- 1,477) |
| competitor_load/dotenv/plain/10k | 5,833,813 ns/iter (+/- 25,508) |
| competitor_load/dotenv-flow/plain/10k | 5,867,229 ns/iter (+/- 26,029) |
| competitor_load/dotenvs/plain/10k | 4,531,413 ns/iter (+/- 16,223) |
| competitor_load/dotenvy/plain/10k | 6,591,656 ns/iter (+/- 22,296) |
| competitor_load/adotenv/long-line/10k | 2,428,790 ns/iter (+/- 6,271) |
| competitor_load/adotenv-compact/long-line/10k | 1,622,913 ns/iter (+/- 2,363) |
| competitor_load/dotenv/long-line/10k | 11,082,207 ns/iter (+/- 67,019) |
| competitor_load/dotenv-flow/long-line/10k | 11,458,159 ns/iter (+/- 50,425) |
| competitor_load/dotenvs/long-line/10k | 7,528,034 ns/iter (+/- 30,956) |
| competitor_load/dotenvy/long-line/10k | 13,985,320 ns/iter (+/- 78,169) |
| competitor_load/adotenv/empty/10k | 799 ns/iter (+/- 16) |
| competitor_load/adotenv-compact/empty/10k | 809 ns/iter (+/- 20) |
| competitor_load/dotenv/empty/10k | 2,396 ns/iter (+/- 46) |
| competitor_load/dotenv-flow/empty/10k | 2,513 ns/iter (+/- 56) |
| competitor_load/dotenvs/empty/10k | 3,273 ns/iter (+/- 86) |
| competitor_load/dotenvy/empty/10k | 2,515 ns/iter (+/- 55) |
| competitor_load/adotenv/plain/100k | 19,579,963 ns/iter (+/- 69,497) |
| competitor_load/adotenv-compact/plain/100k | 10,087,088 ns/iter (+/- 29,546) |
| competitor_load/dotenv/plain/100k | 86,295,143 ns/iter (+/- 1,112,396) |
| competitor_load/dotenv-flow/plain/100k | 86,390,445 ns/iter (+/- 208,709) |
| competitor_load/dotenvs/plain/100k | 63,043,155 ns/iter (+/- 214,410) |
| competitor_load/dotenvy/plain/100k | 98,063,904 ns/iter (+/- 1,180,028) |
| competitor_load/adotenv/long-line/100k | 27,365,833 ns/iter (+/- 96,810) |
| competitor_load/adotenv-compact/long-line/100k | 17,537,562 ns/iter (+/- 66,344) |
| competitor_load/dotenv/long-line/100k | 145,084,284 ns/iter (+/- 503,546) |
| competitor_load/dotenv-flow/long-line/100k | 148,518,616 ns/iter (+/- 395,529) |
| competitor_load/dotenvs/long-line/100k | 95,051,387 ns/iter (+/- 331,287) |
| competitor_load/dotenvy/long-line/100k | 173,317,779 ns/iter (+/- 197,086) |
| competitor_load/adotenv/empty/100k | 798 ns/iter (+/- 6) |
| competitor_load/adotenv-compact/empty/100k | 843 ns/iter (+/- 19) |
| competitor_load/dotenv/empty/100k | 2,420 ns/iter (+/- 45) |
| competitor_load/dotenv-flow/empty/100k | 2,411 ns/iter (+/- 1) |
| competitor_load/dotenvs/empty/100k | 3,417 ns/iter (+/- 51) |
| competitor_load/dotenvy/empty/100k | 2,512 ns/iter (+/- 57) |
