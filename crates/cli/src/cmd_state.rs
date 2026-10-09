use super::open_ctx;
use anyhow::Result;

pub(crate) fn cmd_task(title: String) -> Result<()> {
    let (ctx, _) = open_ctx()?;
    let mut state = ctx.load_state()?;
    state.tasks.push(ctx_core::Task {
        title,
        status: "pending".into(),
        notes: None,
    });
    ctx.save_state(&state)?;
    println!("Task added.");
    Ok(())
}

pub(crate) fn cmd_decide(
    text: String,
    reason: Option<String>,
    supersedes: Option<String>,
) -> Result<()> {
    let (ctx, _) = open_ctx()?;
    let mut state = ctx.load_state()?;
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
    let mut flagged: Vec<String> = Vec::new();
    for d in &state.decisions {
        if !d.active() || Some(d.id.clone()) == supersedes {
            continue;
        }
        if ctx_core::maybe_conflicts(&d.decision, &text) {
            flagged.push(d.id.clone());
        }
    }

    state.decisions.push(ctx_core::Decision {
        id: id.clone(),
        decision: text,
        reason,
        recorded_at: Some(chrono::Utc::now().to_rfc3339()),
        superseded_by: None,
        conflicts_with: flagged.first().cloned(),
    });
    ctx.save_state(&state)?;

    if flagged.is_empty() {
        println!("Decision {} recorded.", id);
    } else {
        println!("Decision {} recorded with a possible conflict:", id);
        for f in &flagged {
            if let Some(old) = state.decisions.iter().find(|d| &d.id == f) {
                println!("  ⚠ {} says: {}", old.id, old.decision);
            }
        }
        println!("Review with `ctx inspect decisions`; resolve with");
        println!(
            "`ctx decide --supersedes <id> ...` or `ctx resolve {}`.",
            id
        );
    }
    Ok(())
}

pub(crate) fn cmd_resolve(id: String) -> Result<()> {
    let (ctx, _) = open_ctx()?;
    let mut state = ctx.load_state()?;
    match state.decisions.iter_mut().find(|d| d.id == id) {
        Some(d) => {
            d.conflicts_with = None;
            ctx.save_state(&state)?;
            println!("{} marked reviewed — conflict flag cleared.", id);
            Ok(())
        }
        None => anyhow::bail!("no decision {} (`ctx inspect decisions` to list)", id),
    }
}

pub(crate) fn cmd_complete(item: String) -> Result<()> {
    let (ctx, _) = open_ctx()?;
    let mut state = ctx.load_state()?;
    state.completed.push(item);
    ctx.save_state(&state)?;
    println!("Marked complete.");
    Ok(())
}

pub(crate) fn cmd_next(action: String) -> Result<()> {
    let (ctx, _) = open_ctx()?;
    let mut state = ctx.load_state()?;
    state.next_action = Some(action.clone());
    ctx.save_state(&state)?;
    println!("Next action set: {}", action);
    Ok(())
}

pub(crate) fn cmd_objective(text: String) -> Result<()> {
    let (ctx, _) = open_ctx()?;
    let mut state = ctx.load_state()?;
    state.objective = Some(text.clone());
    state.status = "in_progress".into();
    ctx.save_state(&state)?;
    println!("Objective set: {}", text);
    Ok(())
}
