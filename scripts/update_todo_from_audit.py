import json
import os
import sys
from datetime import datetime

TODO_FILE = "TODO.md"
AUDIT_DIR = "docs/audits"

def update_todo(audit_file):
    try:
        with open(audit_file, 'r') as f:
            data = json.load(f)
        
        results = data.get("runs", [{}])[0].get("results", [])
        issue_count = len(results)
        
        # Determine severity breakdown
        severities = {}
        for res in results:
            sev = res.get("level", "warning")
            severities[sev] = severities.get(sev, 0) + 1
        
        sev_str = ", ".join([f"{count} {sev}" for sev, count in severities.items()])
        
        filename = os.path.basename(audit_file)
        now = datetime.now().strftime("%Y-%m-%d %H:%M")
        
        task_title = f"Review audit findings from `{filename}`"
        task_desc = f"Audit dropped at {now}. Found {issue_count} issues ({sev_str})."
        
        new_entry = f"\n- [ ] **{task_title}** — {task_desc}\n"
        new_entry += f"  - File: `{audit_file}`\n"

        if not os.path.exists(TODO_FILE):
            with open(TODO_FILE, 'w') as f:
                f.write("# TODO\n\n## Audits\n" + new_entry)
            return

        with open(TODO_FILE, 'r') as f:
            lines = f.readlines()

        # Check if an entry for this file already exists
        filename = os.path.basename(audit_file)
        if any(f"Review audit findings from `{filename}`" in line for line in lines):
            print(f"Entry for {filename} already exists in {TODO_FILE}")
            return

        # Find the "Audits" section or "P2" or something
        audits_section_idx = -1
        for i, line in enumerate(lines):
            if "## Audits" in line:
                audits_section_idx = i
                break
        
        if audits_section_idx != -1:
            lines.insert(audits_section_idx + 1, new_entry)
        else:
            # Add after "## P2" or at the end
            p2_idx = -1
            for i, line in enumerate(lines):
                if "## P2" in line:
                    p2_idx = i
            
            if p2_idx != -1:
                # Find the next section or empty line
                insert_idx = p2_idx + 1
                while insert_idx < len(lines) and not lines[insert_idx].startswith("##"):
                    insert_idx += 1
                lines.insert(insert_idx, "\n## Audits\n" + new_entry)
            else:
                lines.append("\n## Audits\n" + new_entry)

        with open(TODO_FILE, 'w') as f:
            f.writelines(lines)
            
        print(f"Updated {TODO_FILE} with findings from {filename}")

    except Exception as e:
        print(f"Error updating TODO.md: {e}")

if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("Usage: python scripts/update_todo_from_audit.py <audit_file>")
        sys.exit(1)
    
    update_todo(sys.argv[1])
