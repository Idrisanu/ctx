use super::open_ctx;
use anyhow::Result;

pub(crate) fn cmd_task(title: String) -> Result<()> {
    let (ctx, _) = open_ctx()?;
    ctx.update_state(|state| {
        state.tasks.push(ctx_core::Task {
            title: title.clone(),
            status: "pending".into(),
            notes: None,
        });
        Ok(())
    })?;
    println!("Task added.");
    Ok(())
}

pub(crate) fn cmd_decide(
    text: String,
    reason: Option<String>,
    supersedes: Option<String>,
) -> Result<()> {
    let (ctx, _) = open_ctx()?;
    let mut new_id = String::new();
    let mut flagged: Vec<String> = Vec::new();
    ctx.update_state(|state| {
        let id = format!("CTX-{}", state.decisions.len() + 1);

        // Explicit replacement: retire the old decision, keep it in history.
        if let Some(old_id) = supersedes.as_deref() {
            match state.decisions.iter_mut().find(|d| d.id == old_id) {
                Some(old) => {
                    old.superseded_by = Some(id.clone());
                    old.conflicts_with = None;
                }
                None => anyhow::bail!(
                    "no decision {} to supersede (`ctx inspect decisions` to list)",
                    old_id
                ),
            }
        }

        // Guardrail: flag possible contradictions with still-active decisions.
        for d in state.decisions.iter() {
            if !d.active() || Some(d.id.clone()) == supersedes {
                continue;
            }
            if ctx_core::maybe_conflicts(&d.decision, &text) {
                flagged.push(d.id.clone());
            }
        }

        state.decisions.push(ctx_core::Decision {
            id: id.clone(),
            decision: text.clone(),
            reason: reason.clone(),
            recorded_at: Some(chrono::Utc::now().to_rfc3339()),
            superseded_by: None,
            conflicts_with: flagged.first().cloned(),
        });
        new_id = id;
        Ok(())
    })?;

    if flagged.is_empty() {
        println!("Decision {} recorded.", new_id);
    } else {
        println!("Decision {} recorded with a possible conflict:", new_id);
        // Re-read for display (state already saved under lock).
        let (ctx, _) = open_ctx()?;
        if let Ok(state) = ctx.load_state() {
            for f in &flagged {
                if let Some(old) = state.decisions.iter().find(|d| &d.id == f) {
                    println!("  ⚠ {} says: {}", old.id, old.decision);
                }
            }
        }
        println!("Review with `ctx inspect decisions`; resolve with");
        println!(
            "`ctx decide --supersedes <id> ...` or `ctx resolve {}`.",
            new_id
        );
    }
    Ok(())
}

pub(crate) fn cmd_resolve(id: String) -> Result<()> {
    let (ctx, _) = open_ctx()?;
    let id2 = id.clone();
    ctx.update_state(
        |state| match state.decisions.iter_mut().find(|d| d.id == id2) {
            Some(d) => {
                d.conflicts_with = None;
                Ok(())
            }
            None => anyhow::bail!("no decision {} (`ctx inspect decisions` to list)", id2),
        },
    )?;
    println!("{} marked reviewed — conflict flag cleared.", id);
    Ok(())
}

pub(crate) fn cmd_complete(item: String) -> Result<()> {
    let (ctx, _) = open_ctx()?;
    ctx.update_state(|state| {
        state.completed.push(item.clone());
        Ok(())
    })?;
    println!("Marked complete.");
    Ok(())
}

pub(crate) fn cmd_next(action: String) -> Result<()> {
    let (ctx, _) = open_ctx()?;
    ctx.update_state(|state| {
        state.next_action = Some(action.clone());
        Ok(())
    })?;
    println!("Next action set: {}", action);
    Ok(())
}

pub(crate) fn cmd_objective(text: String) -> Result<()> {
    let (ctx, _) = open_ctx()?;
    ctx.update_state(|state| {
        state.objective = Some(text.clone());
        state.status = "in_progress".into();
        Ok(())
    })?;
    println!("Objective set: {}", text);
    Ok(())
}
