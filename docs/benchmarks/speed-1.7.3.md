# Speed benchmark results

Click a column header to sort. Use the filter to narrow large result sets.

<details>
<summary>Provenance fingerprint</summary>

```json
{
  "arch": "x86_64",
  "cpu": "Intel(R) Core(TM) i7-6700HQ CPU @ 2.60GHz",
  "cpu_count": 8,
  "git_sha": "c5c199fd102a09c58cf7e28d8695335c190f3570",
  "hostname": "unknown",
  "hyperfine": "hyperfine 2.0.0",
  "kernel": "Linux 7.1.3-arch1-2",
  "load1": "0.13",
  "os": "linux",
  "rustc": "rustc 1.97.1 (8bab26f4f 2026-07-14)"
}
```

</details>

| benchmark | time |
|---|---:|
| competitor_values/adotenv/plain/8 | 930 ns/iter (+/- 0) |
| competitor_values/adotenv-compact/plain/8 | 892 ns/iter (+/- 1) |
| competitor_values/adotenv-compact-entries/plain/8 | 865 ns/iter (+/- 2) |
| competitor_values/dotenv-parser/plain/8 | 13,923 ns/iter (+/- 10) |
| competitor_values/dotenvs/plain/8 | 3,238 ns/iter (+/- 13) |
| competitor_values/dotenvy/plain/8 | 4,575 ns/iter (+/- 9) |
| competitor_values/adotenv/long-line/8 | 1,661 ns/iter (+/- 2) |
| competitor_values/adotenv-compact/long-line/8 | 1,441 ns/iter (+/- 1) |
| competitor_values/adotenv-compact-entries/long-line/8 | 1,430 ns/iter (+/- 1) |
| competitor_values/dotenv-parser/long-line/8 | 58,773 ns/iter (+/- 69) |
| competitor_values/dotenvs/long-line/8 | 4,734 ns/iter (+/- 13) |
| competitor_values/dotenvy/long-line/8 | 10,070 ns/iter (+/- 40) |
| competitor_values/adotenv/plain/128 | 22,675 ns/iter (+/- 39) |
| competitor_values/adotenv-compact/plain/128 | 12,064 ns/iter (+/- 12) |
| competitor_values/adotenv-compact-entries/plain/128 | 11,809 ns/iter (+/- 17) |
| competitor_values/dotenv-parser/plain/128 | 226,804 ns/iter (+/- 921) |
| competitor_values/dotenvs/plain/128 | 58,773 ns/iter (+/- 158) |
| competitor_values/dotenvy/plain/128 | 82,873 ns/iter (+/- 1,669) |
| competitor_values/adotenv/long-line/128 | 30,247 ns/iter (+/- 71) |
| competitor_values/adotenv-compact/long-line/128 | 20,453 ns/iter (+/- 36) |
| competitor_values/adotenv-compact-entries/long-line/128 | 20,190 ns/iter (+/- 7) |
| competitor_values/dotenv-parser/long-line/128 | 934,979 ns/iter (+/- 885) |
| competitor_values/dotenvs/long-line/128 | 88,735 ns/iter (+/- 213) |
| competitor_values/dotenvy/long-line/128 | 173,250 ns/iter (+/- 132) |
| competitor_values/adotenv/plain/1k | 173,230 ns/iter (+/- 141) |
| competitor_values/adotenv-compact/plain/1k | 90,276 ns/iter (+/- 48) |
| competitor_values/adotenv-compact-entries/plain/1k | 88,849 ns/iter (+/- 45) |
| competitor_values/dotenv-parser/plain/1k | 1,890,238 ns/iter (+/- 1,337) |
| competitor_values/dotenvs/plain/1k | 444,174 ns/iter (+/- 3,029) |
| competitor_values/dotenvy/plain/1k | 647,266 ns/iter (+/- 6,845) |
| competitor_values/adotenv/long-line/1k | 233,772 ns/iter (+/- 180) |
| competitor_values/adotenv-compact/long-line/1k | 154,376 ns/iter (+/- 58) |
| competitor_values/adotenv-compact-entries/long-line/1k | 152,761 ns/iter (+/- 75) |
| competitor_values/dotenv-parser/long-line/1k | 7,349,024 ns/iter (+/- 6,649) |
| competitor_values/dotenvs/long-line/1k | 682,427 ns/iter (+/- 5,348) |
| competitor_values/dotenvy/long-line/1k | 1,331,243 ns/iter (+/- 1,183) |
| competitor_values/adotenv/plain/10k | 1,744,359 ns/iter (+/- 1,855) |
| competitor_values/adotenv-compact/plain/10k | 916,737 ns/iter (+/- 771) |
| competitor_values/adotenv-compact-entries/plain/10k | 900,608 ns/iter (+/- 565) |
| competitor_values/dotenv-parser/plain/10k | 20,444,186 ns/iter (+/- 55,985) |
| competitor_values/dotenvs/plain/10k | 4,453,794 ns/iter (+/- 27,142) |
| competitor_values/dotenvy/plain/10k | 6,481,860 ns/iter (+/- 56,459) |
| competitor_values/adotenv/long-line/10k | 2,334,170 ns/iter (+/- 2,412) |
| competitor_values/adotenv-compact/long-line/10k | 1,538,902 ns/iter (+/- 9,443) |
| competitor_values/adotenv-compact-entries/long-line/10k | 1,522,818 ns/iter (+/- 9,540) |
| competitor_values/dotenv-parser/long-line/10k | 74,096,250 ns/iter (+/- 137,264) |
| competitor_values/dotenvs/long-line/10k | 7,291,765 ns/iter (+/- 29,578) |
| competitor_values/dotenvy/long-line/10k | 13,501,598 ns/iter (+/- 42,323) |
| competitor_values/adotenv/plain/100k | 19,055,749 ns/iter (+/- 71,726) |
| competitor_values/adotenv-compact/plain/100k | 10,106,457 ns/iter (+/- 15,832) |
| competitor_values/adotenv-compact-entries/plain/100k | 9,966,647 ns/iter (+/- 11,458) |
| competitor_values/dotenv-parser/plain/100k | 227,565,222 ns/iter (+/- 708,786) |
| competitor_values/dotenvs/plain/100k | 62,537,915 ns/iter (+/- 167,973) |
| competitor_values/dotenvy/plain/100k | 98,260,452 ns/iter (+/- 1,074,108) |
| competitor_values/adotenv/long-line/100k | 25,552,002 ns/iter (+/- 41,237) |
| competitor_values/adotenv-compact/long-line/100k | 15,776,788 ns/iter (+/- 18,016) |
| competitor_values/adotenv-compact-entries/long-line/100k | 15,623,911 ns/iter (+/- 17,593) |
| competitor_values/dotenv-parser/long-line/100k | 755,763,300 ns/iter (+/- 895,710) |
| competitor_values/dotenvs/long-line/100k | 94,202,238 ns/iter (+/- 478,630) |
| competitor_values/dotenvy/long-line/100k | 169,306,477 ns/iter (+/- 196,380) |
| competitor_load/adotenv/plain/8 | 6,440 ns/iter (+/- 126) |
| competitor_load/adotenv-compact/plain/8 | 6,320 ns/iter (+/- 128) |
| competitor_load/dotenv/plain/8 | 7,103 ns/iter (+/- 83) |
| competitor_load/dotenv-flow/plain/8 | 7,185 ns/iter (+/- 77) |
| competitor_load/dotenvs/plain/8 | 7,250 ns/iter (+/- 77) |
| competitor_load/dotenvy/plain/8 | 7,548 ns/iter (+/- 79) |
| competitor_load/adotenv/long-line/8 | 6,946 ns/iter (+/- 103) |
| competitor_load/adotenv-compact/long-line/8 | 6,790 ns/iter (+/- 126) |
| competitor_load/dotenv/long-line/8 | 11,112 ns/iter (+/- 49) |
| competitor_load/dotenv-flow/long-line/8 | 10,540 ns/iter (+/- 68) |
| competitor_load/dotenvs/long-line/8 | 8,837 ns/iter (+/- 65) |
| competitor_load/dotenvy/long-line/8 | 12,845 ns/iter (+/- 195) |
| competitor_load/adotenv/empty/8 | 4,615 ns/iter (+/- 105) |
| competitor_load/adotenv-compact/empty/8 | 4,439 ns/iter (+/- 105) |
| competitor_load/dotenv/empty/8 | 2,462 ns/iter (+/- 44) |
| competitor_load/dotenv-flow/empty/8 | 2,538 ns/iter (+/- 55) |
| competitor_load/dotenvs/empty/8 | 3,470 ns/iter (+/- 65) |
| competitor_load/dotenvy/empty/8 | 2,576 ns/iter (+/- 50) |
| competitor_load/adotenv/plain/128 | 29,020 ns/iter (+/- 134) |
| competitor_load/adotenv-compact/plain/128 | 17,683 ns/iter (+/- 109) |
| competitor_load/dotenv/plain/128 | 78,820 ns/iter (+/- 86) |
| competitor_load/dotenv-flow/plain/128 | 78,900 ns/iter (+/- 69) |
| competitor_load/dotenvs/plain/128 | 63,539 ns/iter (+/- 69) |
| competitor_load/dotenvy/plain/128 | 86,198 ns/iter (+/- 106) |
| competitor_load/adotenv/long-line/128 | 37,903 ns/iter (+/- 105) |
| competitor_load/adotenv-compact/long-line/128 | 27,107 ns/iter (+/- 130) |
| competitor_load/dotenv/long-line/128 | 148,580 ns/iter (+/- 138) |
| competitor_load/dotenv-flow/long-line/128 | 143,911 ns/iter (+/- 128) |
| competitor_load/dotenvs/long-line/128 | 94,305 ns/iter (+/- 98) |
| competitor_load/dotenvy/long-line/128 | 177,752 ns/iter (+/- 164) |
| competitor_load/adotenv/empty/128 | 4,641 ns/iter (+/- 94) |
| competitor_load/adotenv-compact/empty/128 | 4,644 ns/iter (+/- 102) |
| competitor_load/dotenv/empty/128 | 2,482 ns/iter (+/- 5) |
| competitor_load/dotenv-flow/empty/128 | 2,512 ns/iter (+/- 61) |
| competitor_load/dotenvs/empty/128 | 3,389 ns/iter (+/- 61) |
| competitor_load/dotenvy/empty/128 | 2,492 ns/iter (+/- 61) |
| competitor_load/adotenv/plain/1k | 182,895 ns/iter (+/- 252) |
| competitor_load/adotenv-compact/plain/1k | 98,879 ns/iter (+/- 143) |
| competitor_load/dotenv/plain/1k | 589,152 ns/iter (+/- 645) |
| competitor_load/dotenv-flow/plain/1k | 591,282 ns/iter (+/- 953) |
| competitor_load/dotenvs/plain/1k | 449,941 ns/iter (+/- 455) |
| competitor_load/dotenvy/plain/1k | 644,503 ns/iter (+/- 14,541) |
| competitor_load/adotenv/long-line/1k | 252,036 ns/iter (+/- 717) |
| competitor_load/adotenv-compact/long-line/1k | 171,092 ns/iter (+/- 125) |
| competitor_load/dotenv/long-line/1k | 1,123,083 ns/iter (+/- 9,712) |
| competitor_load/dotenv-flow/long-line/1k | 1,080,321 ns/iter (+/- 17,205) |
| competitor_load/dotenvs/long-line/1k | 690,785 ns/iter (+/- 1,582) |
| competitor_load/dotenvy/long-line/1k | 1,335,085 ns/iter (+/- 1,268) |
| competitor_load/adotenv/empty/1k | 4,420 ns/iter (+/- 94) |
| competitor_load/adotenv-compact/empty/1k | 4,630 ns/iter (+/- 97) |
| competitor_load/dotenv/empty/1k | 2,598 ns/iter (+/- 59) |
| competitor_load/dotenv-flow/empty/1k | 2,518 ns/iter (+/- 60) |
| competitor_load/dotenvs/empty/1k | 3,489 ns/iter (+/- 61) |
| competitor_load/dotenvy/empty/1k | 2,574 ns/iter (+/- 54) |
| competitor_load/adotenv/plain/10k | 1,772,309 ns/iter (+/- 2,796) |
| competitor_load/adotenv-compact/plain/10k | 945,620 ns/iter (+/- 794) |
| competitor_load/dotenv/plain/10k | 5,754,148 ns/iter (+/- 26,105) |
| competitor_load/dotenv-flow/plain/10k | 5,789,355 ns/iter (+/- 13,897) |
| competitor_load/dotenvs/plain/10k | 4,446,793 ns/iter (+/- 6,946) |
| competitor_load/dotenvy/plain/10k | 6,477,018 ns/iter (+/- 15,410) |
| competitor_load/adotenv/long-line/10k | 2,449,558 ns/iter (+/- 8,103) |
| competitor_load/adotenv-compact/long-line/10k | 1,647,561 ns/iter (+/- 1,622) |
| competitor_load/dotenv/long-line/10k | 11,284,997 ns/iter (+/- 70,307) |
| competitor_load/dotenv-flow/long-line/10k | 10,927,942 ns/iter (+/- 58,575) |
| competitor_load/dotenvs/long-line/10k | 7,344,045 ns/iter (+/- 29,413) |
| competitor_load/dotenvy/long-line/10k | 13,535,022 ns/iter (+/- 68,581) |
| competitor_load/adotenv/empty/10k | 4,659 ns/iter (+/- 100) |
| competitor_load/adotenv-compact/empty/10k | 4,425 ns/iter (+/- 11) |
| competitor_load/dotenv/empty/10k | 2,603 ns/iter (+/- 54) |
| competitor_load/dotenv-flow/empty/10k | 2,469 ns/iter (+/- 47) |
| competitor_load/dotenvs/empty/10k | 3,370 ns/iter (+/- 62) |
| competitor_load/dotenvy/empty/10k | 2,579 ns/iter (+/- 26) |
| competitor_load/adotenv/plain/100k | 19,671,154 ns/iter (+/- 67,914) |
| competitor_load/adotenv-compact/plain/100k | 10,032,451 ns/iter (+/- 38,670) |
| competitor_load/dotenv/plain/100k | 91,338,303 ns/iter (+/- 469,984) |
| competitor_load/dotenv-flow/plain/100k | 91,881,568 ns/iter (+/- 398,407) |
| competitor_load/dotenvs/plain/100k | 63,720,675 ns/iter (+/- 218,523) |
| competitor_load/dotenvy/plain/100k | 99,009,262 ns/iter (+/- 699,111) |
| competitor_load/adotenv/long-line/100k | 27,600,726 ns/iter (+/- 107,632) |
| competitor_load/adotenv-compact/long-line/100k | 17,713,413 ns/iter (+/- 48,743) |
| competitor_load/dotenv/long-line/100k | 148,986,355 ns/iter (+/- 325,061) |
| competitor_load/dotenv-flow/long-line/100k | 145,101,197 ns/iter (+/- 940,681) |
| competitor_load/dotenvs/long-line/100k | 94,546,422 ns/iter (+/- 212,121) |
| competitor_load/dotenvy/long-line/100k | 170,592,206 ns/iter (+/- 335,394) |
| competitor_load/adotenv/empty/100k | 4,632 ns/iter (+/- 98) |
| competitor_load/adotenv-compact/empty/100k | 4,649 ns/iter (+/- 89) |
| competitor_load/dotenv/empty/100k | 2,582 ns/iter (+/- 58) |
| competitor_load/dotenv-flow/empty/100k | 2,510 ns/iter (+/- 57) |
| competitor_load/dotenvs/empty/100k | 3,391 ns/iter (+/- 67) |
| competitor_load/dotenvy/empty/100k | 2,497 ns/iter (+/- 58) |
