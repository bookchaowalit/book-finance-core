---
title: Book Finance Core — Product Card
doc_id: CARD-book-finance-core
doc_type: product-card
verdict: showcase
version: 0.1
date: 2026-10-08
status: ร่างเพื่อพิจารณา
owner: Product owner (Book Dev)
---

# Book Finance Core — Product Card

## 1. สรุปคุณค่าในหนึ่งประโยค

kernel ภาษา Rust ขนาดเล็กที่บังคับ invariant ของจำนวนเงินและบัญชีคู่ (double-entry) เพื่อเป็นฐานที่ปลอดภัยของ Finance Platform ใน Solo Empire

## 2. ปัญหาและผู้ใช้เป้าหมาย

- ระบบการเงินภายในต้องการกฎเงิน/บัญชีที่ผิดไม่ได้ เช่นสกุลเงินผสมหรือบัญชีไม่สมดุล
- ผู้ใช้: ระบบ finance ของ Solo Empire (shadow parser ของ API `infra/api/finance.ts`)

## 3. สิ่งที่ทำได้จริงตอนนี้

- `src/lib.rs`: `Currency`, `Money`, `Posting`, `LedgerEntry`, `LedgerBook`, `DomainError`
- binary `finance-v1-shadow` parse contract `finance.v1` แบบอ่านอย่างเดียว; `finance-command-pilot` เปิดเฉพาะเมื่อ `FINANCE_CORE_COMMANDS_ENABLED=true` และทำได้แค่ preview
- JSON Schema `fixtures/finance-v1.schema.json` พร้อม test กัน drift; property tests และ currency scale tests
- ไม่มี DB ไม่มีเครือข่าย ไม่เขียนข้อมูล

## 4. หลักฐานสถานะ

- Stack: Rust
- Commits: 7 (แรก 2026-09-09 → ล่าสุด 2026-10-01), branch `main`; working tree สะอาด
- Source: 10 ไฟล์ / 2,217 บรรทัด; test 5 ไฟล์
- CI: 1 workflow
- Deploy config: ไม่มี
- Live URL: ไม่พบใน repo
- CI รัน tests, rustfmt, clippy `-D warnings` ผ่าน `scripts/check.sh`

## 5. คุณค่าและจุดต่าง

- เป็นหลักฐานการออกแบบ domain model ที่ถูกต้องเชิงการเงิน (type safety, property test)
- ยังไม่ใช่ผลิตภัณฑ์: README ระบุว่าเป็น scaffold และไม่ใช่ production cutover; ไม่มีมุมรายได้

## 6. ช่องว่างสำคัญ

- ใช้ในโหมด shadow เท่านั้น ยังไม่มีเส้นทางเขียนจริง
- commit เพียง 7 ครั้ง ยังไม่มีแผนชัดว่าจะแทน logic ฝั่ง TypeScript เมื่อใด **[ต้องยืนยัน]**

## 7. คำตัดสิน

**showcase** — โค้ดดีและมีวินัยสูง แต่เป็นชิ้นส่วนภายในที่ยังอยู่ในขั้น shadow จึงเป็นหลักฐานความสามารถมากกว่าผลิตภัณฑ์

จะเป็น `invest` ถ้าตัดสินใจย้าย ledger ของ Solo Empire มาใช้ core นี้จริงและมีแผน cutover

## 8. ขั้นต่อไป

1. ตัดสินใจว่าจะ cutover หรือคงเป็น shadow ถาวร
2. ถ้าไม่ cutover ให้คงไว้เป็นตัวอย่าง Rust domain modelling

## การอนุมัติ

| บทบาท | สถานะ | วันที่ | ลงนาม |
|---|---|---|---|
| Product owner (Book Dev) | รอพิจารณา | — | ________ |

## แหล่งข้อมูล

- `README.md`
- `src/lib.rs`
- `src/bin/`
- `fixtures/finance-v1.schema.json`
- `tests/`
