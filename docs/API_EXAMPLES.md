# API Examples

Set `ADMIN_API_KEY` in `.env` and pass it as `x-admin-api-key`.

## Lead Qualification

```bash
curl -X POST http://localhost:8000/agents/lead-qualification \
  -H "content-type: application/json" \
  -H "x-admin-api-key: change-me-admin-key" \
  -d '{"source":"manual","email":"founder@example.com","name":"Avery Stone","company":"Example AI","title":"Founder"}'
```

## Client Onboarding

```bash
curl -X POST http://localhost:8000/agents/client-onboarding \
  -H "content-type: application/json" \
  -H "x-admin-api-key: change-me-admin-key" \
  -d '{"client_name":"Example Co","client_email":"ops@example.com","contract_id":"contract-123","project_type":"implementation"}'
```

## Delivery Monitoring

```bash
curl -X POST http://localhost:8000/agents/delivery-monitoring \
  -H "content-type: application/json" \
  -H "x-admin-api-key: change-me-admin-key" \
  -d '{"project_id":"proj-1","client_name":"Example Co","milestone":"Phase 1","completion_pct":35,"budget_used_pct":72,"days_since_client_contact":9}'
```

## Finance Operations

```bash
curl -X POST http://localhost:8000/agents/finance-operations \
  -H "content-type: application/json" \
  -H "x-admin-api-key: change-me-admin-key" \
  -d '{"customer_id":"cus_123","customer_email":"billing@example.com","amount_cents":250000,"currency":"usd","description":"Monthly retainer"}'
```
