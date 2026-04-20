# Phase 5: Facter Integration

## Completed Tasks

### 5.1 Facter Data Management - COMPLETE
- [x] Implement Facter data parser (FacterService)
- [x] Support for core facts (via PuppetDB integration)
- [x] Support for custom facts (FactTemplate with Static, FromFact, FromClassification, Template sources)
- [x] Support for external facts (fact generation and export)
- [x] Fact template CRUD API endpoints
- [x] Fact generation from templates
- [x] Export formats: JSON, YAML, Shell script
- [x] Frontend Facter Templates management page
- [ ] Fact history tracking (future enhancement)
- [ ] Integrate classification with facts (classification should be able to generate custom facts)

### 5.2 Facter Generation - COMPLETE
- [x] Design facter generation templates (FactTemplate model with FactDefinition and FactValueSource)
- [x] Generate external facts from classifications (FromClassification source type)
- [x] Export facts in JSON/YAML formats (plus Shell format)
- [x] Fact validation and schema enforcement:
  - Template name validation (alphanumeric, underscores, hyphens)
  - Fact name validation (lowercase, underscores only - Puppet/Facter compatible)
  - Fact value source validation (Static, FromClassification, FromFact, Template)
  - Template string syntax validation (balanced braces)
  - Duplicate fact name detection
  - Size limits for values and templates

### 5.3 API Endpoints - COMPLETE
- [x] GET /api/v1/facter/templates - List fact templates
- [x] GET /api/v1/facter/templates/:id - Get fact template
- [x] POST /api/v1/facter/templates - Create fact template
- [x] PUT /api/v1/facter/templates/:id - Update fact template
- [x] DELETE /api/v1/facter/templates/:id - Delete fact template
- [x] POST /api/v1/facter/generate - Generate facts for node
- [x] GET /api/v1/facter/export/:certname - Export node facts

## Details

Complete integration with Facter for custom and external fact management:

### Fact Template System

**FactDefinition:**
- name: Fact name (lowercase, underscores only)
- value_source: How to generate the value
- description: Optional description

**FactValueSource Types:**

1. **Static:** Fixed value
   ```json
   {
     "type": "Static",
     "value": "fixed_value"
   }
   ```

2. **FromFact:** Copy from another fact
   ```json
   {
     "type": "FromFact",
     "source_fact": "os.family"
   }
   ```

3. **FromClassification:** Get value from classification
   ```json
   {
     "type": "FromClassification",
     "classification_key": "environment"
   }
   ```

4. **Template:** Use template string with variable substitution
   ```json
   {
     "type": "Template",
     "template": "node-{os.family}-{processors.count}"
   }
   ```

### Export Formats

**JSON Export:**
```json
{
  "custom_environment": "production",
  "node_role": "web_server",
  "deployment_id": "node-Linux-8"
}
```

**YAML Export:**
```yaml
custom_environment: production
node_role: web_server
deployment_id: node-Linux-8
```

**Shell Export:**
```bash
export FACTER_CUSTOM_ENVIRONMENT="production"
export FACTER_NODE_ROLE="web_server"
export FACTER_DEPLOYMENT_ID="node-Linux-8"
```

### Validation

**Template Name:**
- Alphanumeric characters, underscores, hyphens
- 1-255 characters
- Unique per organization

**Fact Name:**
- Lowercase letters, underscores only
- 1-128 characters
- Puppet/Facter compatible
- No duplicates within template

**Value Source:**
- Valid source type specified
- Required fields present for source type
- Referenced facts/classifications exist
- Template strings have balanced braces

**Size Limits:**
- Fact value max: 64KB
- Fact name max: 128 characters
- Template max: 1MB
- Total facts per template: 1000

### Frontend Components

```
frontend/src/pages/
├── FacterTemplates.tsx          # Templates management page
├── facter/
│   ├── TemplatesList.tsx        # Templates list
│   ├── TemplateEditor.tsx       # Create/edit form
│   ├── FactDefinitionEditor.tsx # Fact definition editing
│   ├── ExportModal.tsx          # Export dialog
│   └── GenerationResults.tsx    # Generation output

frontend/src/hooks/
└── useFacterTemplates.ts        # Template API hooks
```

### API Endpoints

**Template Management:**
```
GET    /api/v1/facter/templates              # List templates
POST   /api/v1/facter/templates              # Create template
GET    /api/v1/facter/templates/:id          # Get template
PUT    /api/v1/facter/templates/:id          # Update template
DELETE /api/v1/facter/templates/:id          # Delete template
```

**Fact Generation:**
```
POST   /api/v1/facter/generate               # Generate facts
GET    /api/v1/facter/export/:certname       # Export facts
```

### Request/Response Examples

**Create Template:**
```json
POST /api/v1/facter/templates

{
  "name": "deployment_facts",
  "description": "Custom deployment facts",
  "facts": [
    {
      "name": "deployment_environment",
      "value_source": {
        "type": "FromClassification",
        "classification_key": "environment"
      },
      "description": "Environment from classification"
    },
    {
      "name": "deployment_id",
      "value_source": {
        "type": "Template",
        "template": "node-{os.family}-{processors.count}"
      },
      "description": "Unique deployment identifier"
    }
  ]
}
```

**Generate Facts:**
```json
POST /api/v1/facter/generate

{
  "certname": "web01.example.com",
  "template_id": "template-uuid",
  "format": "json"
}

Response:
{
  "certname": "web01.example.com",
  "template_id": "template-uuid",
  "format": "json",
  "facts": {
    "deployment_environment": "production",
    "deployment_id": "node-Linux-8"
  },
  "generated_at": "2026-01-22T16:00:00Z"
}
```

**Export Facts:**
```
GET /api/v1/facter/export/web01.example.com?format=yaml&template_id=template-uuid

Response (YAML):
deployment_environment: production
deployment_id: node-Linux-8
```

### Data Models

```rust
pub struct FactTemplate {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub facts: Vec<FactDefinition>,
    pub organization_id: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct FactDefinition {
    pub name: String,
    pub value_source: FactValueSource,
    pub description: Option<String>,
}

pub enum FactValueSource {
    Static(String),
    FromFact(String),
    FromClassification(String),
    Template(String),
}
```

## Future Enhancements

- Fact history tracking
- Fact usage analytics
- Integration with classification engine
- Fact validation rules
- Custom fact plugins

## Key Files

- `src/services/facter.rs` - Facter service
- `src/models/fact_template.rs` - Template model
- `src/repositories/fact_template_repository.rs` - Persistence
- `src/handlers/facter.rs` - API endpoints
- `frontend/src/pages/FacterTemplates.tsx` - UI
