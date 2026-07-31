use hyprland::{
    data::{Client, Clients, Monitor, Monitors},
    dispatch::{Direction, Dispatch, DispatchType},
    keyword::Keyword,
    shared::{HyprData, HyprDataActive, HyprDataActiveOptional},
};

// Import definitions from the command-line interface (CLI).
// `argh` populates these structs with the arguments passed to the program.
use crate::cli::{Command, Flags};

// Declare the `cli` module, which should exist in the `cli.rs` file.
mod cli;

// --- Main Function ---
// Program entry point.
fn main() -> anyhow::Result<()> {
    // 1. Parse the command-line arguments (e.g., "r", "l -s").
    let params: Flags = argh::from_env();
    // 2. Get the list of all open windows and monitors from Hyprland.
    let all_clients = Clients::get()?;
    let all_monitors = Monitors::get()?;

    // 3. Try to get the active window. If there isn't one...
    let Some(active_client) = Client::get_active().ok().flatten() else {
        // ... it means the workspace is empty. Call the function to handle this case.
        return handle_in_empty_ws(&params.cmd);
    };

    // 4. If there is an active window, get information about the current monitor.
    let active_monitor = Monitor::get_active()?;

    // 5. Dispatch execution based on the provided subcommand (r, l, u, d).
    //    It extracts parameters from each command (like `p.swap` and `p.bordersize`)
    //    and passes them to the appropriate handler function.
    match params.cmd {
        Command::Up(p) => handle_vertical_nav(Direction::Up, p.swap, &active_client, &all_clients)?,
        Command::Down(p) => handle_vertical_nav(Direction::Down, p.swap, &active_client, &all_clients)?,
        Command::Left(p) => handle_horizontal_nav(
            Direction::Left,
            p.swap,
            p.bordersize,
            &active_client,
            &active_monitor,
            &all_clients,
            &all_monitors,
        )?,
        Command::Right(p) => handle_horizontal_nav(
            Direction::Right,
            p.swap,
            p.bordersize,
            &active_client,
            &active_monitor,
            &all_clients,
            &all_monitors,
        )?,
    };

    Ok(())
}

// --- Dispatch Helpers for Hyprland 0.55 Lua API ---

fn dir_to_str(direction: Direction) -> &'static str {
    match direction {
        Direction::Up => "up",
        Direction::Down => "down",
        Direction::Left => "left",
        Direction::Right => "right",
    }
}

fn dispatch_focus_workspace(ws: &str) -> anyhow::Result<()> {
    Dispatch::call(DispatchType::Custom(
        "hl.dsp.focus",
        &format!("{{ workspace = \"{}\" }}", ws),
    ))?;
    Ok(())
}

fn dispatch_focus_direction(direction: Direction) -> anyhow::Result<()> {
    let dir = dir_to_str(direction);
    Dispatch::call(DispatchType::Custom(
        "hl.dsp.focus",
        &format!("{{ direction = \"{}\" }}", dir),
    ))?;
    Ok(())
}

fn dispatch_move_direction(direction: Direction) -> anyhow::Result<()> {
    let dir = dir_to_str(direction);
    Dispatch::call(DispatchType::Custom(
        "hl.dsp.window.move",
        &format!("{{ direction = \"{}\" }}", dir),
    ))?;
    Ok(())
}

fn dispatch_move_monitor(direction: Direction) -> anyhow::Result<()> {
    let dir = dir_to_str(direction);
    Dispatch::call(DispatchType::Custom(
        "hl.dsp.window.move",
        &format!("{{ monitor = \"{}\" }}", dir),
    ))?;
    Ok(())
}

fn dispatch_swap_direction(direction: Direction) -> anyhow::Result<()> {
    let dir = dir_to_str(direction);
    Dispatch::call(DispatchType::Custom(
        "hl.dsp.window.swap",
        &format!("{{ direction = \"{}\" }}", dir),
    ))?;
    Ok(())
}

fn dispatch_center_window() -> anyhow::Result<()> {
    Dispatch::call(DispatchType::Custom("hl.dsp.window.center", ""))?;
    Ok(())
}

fn dispatch_focus_window_address(address: &hyprland::shared::Address) -> anyhow::Result<()> {
    Dispatch::call(DispatchType::Custom(
        "hl.dsp.focus",
        &format!("{{ window = \"address:{}\" }}", address),
    ))?;
    Ok(())
}

fn dispatch_cycle_next_tiled() -> anyhow::Result<()> {
    Dispatch::call(DispatchType::Custom(
        "hl.dsp.window.cycle_next",
        "{ tiled = true }",
    ))?;
    Ok(())
}

fn dispatch_cycle_next_floating() -> anyhow::Result<()> {
    Dispatch::call(DispatchType::Custom(
        "hl.dsp.window.cycle_next",
        "{ floating = true }",
    ))?;
    Ok(())
}

fn dispatch_cycle_next_floating_visible(next: bool) -> anyhow::Result<()> {
    Dispatch::call(DispatchType::Custom(
        "hl.dsp.window.cycle_next",
        &format!("{{ next = {}, floating = true }}", next),
    ))?;
    Ok(())
}

fn dispatch_alter_zorder_top() -> anyhow::Result<()> {
    Dispatch::call(DispatchType::Custom(
        "hl.dsp.window.alter_zorder",
        "{ mode = \"top\" }",
    ))?;
    Ok(())
}

// --- Handler Functions ---

/// Handles navigation when there are no windows on the current workspace.
fn handle_in_empty_ws(command: &Command) -> anyhow::Result<()> {
    // Determines whether to go to the previous or next workspace based on the command.
    // "e+1" / "e-1" are Hyprland commands for navigating to the next/previous existing workspace.
    let direction = match command {
        Command::Right(_) | Command::Up(_) => "e+1",
        Command::Left(_) | Command::Down(_) => "e-1",
    };
    dispatch_focus_workspace(direction)?;
    Ok(())
}

/// Handles vertical navigation (Up/Down) with tiled/floating focus switching.
fn handle_vertical_nav(
    direction: Direction,
    swap: bool,
    active_client: &Client,
    all_clients: &Clients,
) -> anyhow::Result<()> {
    if swap {
        // For floating windows, swapping doesn't make sense, so we move the window instead.
        if active_client.floating {
            dispatch_move_direction(direction)?;
        } else {
            // For tiled windows, we use the native swap command.
            dispatch_swap_direction(direction)?;
        }
    } else {
        if active_client.floating {
            if is_extreme_floating(active_client, all_clients, &direction) {
                // Floating window at extreme vertical edge -> focus next tiled window
                dispatch_cycle_next_tiled()?;
            } else {
                // Not at extreme edge -> cycle through floating windows
                let is_down = matches!(direction, Direction::Down);
                dispatch_cycle_next_floating_visible(is_down)?;
            }
            dispatch_alter_zorder_top()?;
        } else {
            // Tiled window vertical move -> check edge boundary
            let tiled_ws_clients: Vec<&Client> = all_clients
                .iter()
                .filter(|c| c.workspace.id == active_client.workspace.id && !c.floating)
                .collect();

            let is_at_vertical_edge = if tiled_ws_clients.is_empty() {
                false
            } else {
                let min_y = tiled_ws_clients
                    .iter()
                    .map(|c| c.at.1)
                    .min()
                    .unwrap_or(active_client.at.1);
                let max_y = tiled_ws_clients
                    .iter()
                    .map(|c| c.at.1)
                    .max()
                    .unwrap_or(active_client.at.1);

                match direction {
                    Direction::Up => active_client.at.1 <= min_y,
                    Direction::Down => active_client.at.1 >= max_y,
                    _ => false,
                }
            };

            if is_at_vertical_edge {
                // At top/bottom edge -> focus next floating window
                dispatch_cycle_next_floating()?;
            } else {
                dispatch_focus_direction(direction)?;
            }
            dispatch_alter_zorder_top()?;
        }
    }
    Ok(())
}

/// Handles horizontal navigation (Left/Right), only switching to an adjacent monitor when at the boundary.
fn handle_horizontal_nav(
    direction: Direction,
    swap: bool,
    bordersize: Option<i32>,
    active_client: &Client,
    active_monitor: &Monitor,
    all_clients: &Clients,
    all_monitors: &Monitors,
) -> anyhow::Result<()> {
    // Determines if we are checking the right or left screen boundary.
    let is_checking_right_bound = match direction {
        Direction::Right => true,
        Direction::Left => false,
        _ => unreachable!(),
    };

    // `is_bound` checks if the active window is physically at the monitor's edge.
    let is_at_boundary = is_bound(
        active_client,
        active_monitor,
        bordersize.unwrap_or(0),
        is_checking_right_bound,
    );

    // Specific logic block for floating windows.
    if active_client.floating {
        if swap {
            // If at the boundary, move the window to the next MONITOR if one exists.
            if is_at_boundary {
                if find_adjacent_monitor(&direction, active_monitor, all_monitors).is_some() {
                    dispatch_move_monitor(direction)?;
                    // Center the window on the new monitor for better placement.
                    dispatch_center_window()?;
                }
            } else {
                // If not at the boundary, just move the window in the specified direction.
                dispatch_move_direction(direction)?;
            }
        } else {
            let is_right = matches!(direction, Direction::Right);
            if !is_extreme_floating(active_client, all_clients, &direction) {
                // Cycle through floating windows until reaching extreme floating window
                dispatch_cycle_next_floating_visible(is_right)?;
                dispatch_alter_zorder_top()?;
            } else {
                // Reached extreme floating window in this direction!
                // Try to focus tiled window on current workspace first
                let find_rightmost = matches!(direction, Direction::Left);
                if let Some((l_client, r_client)) =
                    get_bound_client(all_clients, active_client.workspace.id, false)
                {
                    let target_client = if find_rightmost { r_client } else { l_client };
                    dispatch_focus_window_address(&target_client.address)?;
                } else {
                    // No tiled windows on current workspace -> try adjacent monitor
                    if let Some(target_monitor) =
                        find_adjacent_monitor(&direction, active_monitor, all_monitors)
                    {
                        let target_ws_id = target_monitor.active_workspace.id;
                        if let Some((l_client, r_client)) =
                            get_bound_client(all_clients, target_ws_id, false)
                        {
                            let target_client = if find_rightmost { r_client } else { l_client };
                            dispatch_focus_window_address(&target_client.address)?;
                        } else {
                            // If target monitor's active workspace is empty, focus that workspace
                            dispatch_focus_workspace(&target_ws_id.to_string())?;
                        }
                    }
                    // If no monitor exists in that direction, do nothing!
                }
            }
        }
        return Ok(());
    }

    // Logic block for tiled windows.
    if swap {
        if is_at_boundary {
            // Move window to the adjacent monitor if one exists
            if find_adjacent_monitor(&direction, active_monitor, all_monitors).is_some() {
                dispatch_move_direction(direction)?;
            }
        } else {
            // Otherwise, just swap with the neighboring window on the same workspace.
            dispatch_swap_direction(direction)?;
        }
    } else {
        // Focus logic
        if is_at_boundary {
            // At boundary: only navigate if there is a monitor in that direction
            if let Some(target_monitor) =
                find_adjacent_monitor(&direction, active_monitor, all_monitors)
            {
                let target_ws_id = target_monitor.active_workspace.id;
                let find_rightmost = matches!(direction, Direction::Left);

                if let Some((l_client, r_client)) =
                    get_bound_client(all_clients, target_ws_id, false)
                {
                    let target_client = if find_rightmost { r_client } else { l_client };
                    dispatch_focus_window_address(&target_client.address)?;
                } else {
                    // If target monitor's active workspace is empty, focus that workspace
                    dispatch_focus_workspace(&target_ws_id.to_string())?;
                }
            }
            // If no monitor exists in that direction, do nothing!
        } else {
            // If not at the boundary, just move the focus to the neighboring window.
            dispatch_focus_direction(direction)?;
        }
    }
    Ok(())
}

/// Finds the adjacent monitor in the given direction (Left or Right).
fn find_adjacent_monitor(
    direction: &Direction,
    active_monitor: &Monitor,
    all_monitors: &Monitors,
) -> Option<Monitor> {
    match direction {
        Direction::Left => all_monitors
            .iter()
            .filter(|m| m.x < active_monitor.x)
            .max_by_key(|m| m.x)
            .cloned(),
        Direction::Right => all_monitors
            .iter()
            .filter(|m| m.x > active_monitor.x)
            .min_by_key(|m| m.x)
            .cloned(),
        _ => None,
    }
}

/// Checks if a floating window is the most extreme floating window in a given direction.
fn is_extreme_floating(
    active_client: &Client,
    all_clients: &Clients,
    direction: &Direction,
) -> bool {
    let floating_ws_clients: Vec<&Client> = all_clients
        .iter()
        .filter(|c| c.workspace.id == active_client.workspace.id && c.floating)
        .collect();

    if floating_ws_clients.len() <= 1 {
        return true;
    }

    match direction {
        Direction::Left => {
            let min_x = floating_ws_clients
                .iter()
                .map(|c| c.at.0 as i32)
                .min()
                .unwrap_or(active_client.at.0 as i32);
            active_client.at.0 as i32 <= min_x
        }
        Direction::Right => {
            let max_x = floating_ws_clients
                .iter()
                .map(|c| c.at.0 as i32 + c.size.0 as i32)
                .max()
                .unwrap_or(active_client.at.0 as i32 + active_client.size.0 as i32);
            (active_client.at.0 as i32 + active_client.size.0 as i32) >= max_x
        }
        Direction::Up => {
            let min_y = floating_ws_clients
                .iter()
                .map(|c| c.at.1 as i32)
                .min()
                .unwrap_or(active_client.at.1 as i32);
            active_client.at.1 as i32 <= min_y
        }
        Direction::Down => {
            let max_y = floating_ws_clients
                .iter()
                .map(|c| c.at.1 as i32 + c.size.1 as i32)
                .max()
                .unwrap_or(active_client.at.1 as i32 + active_client.size.1 as i32);
            (active_client.at.1 as i32 + active_client.size.1 as i32) >= max_y
        }
    }
}

// --- Helper Functions ---

/// Checks if a window is physically at the edge of the monitor.
/// This function is crucial as it considers gaps and reserved areas (status bars).
#[inline]
fn is_bound(
    act: &Client,
    monitor: &Monitor,
    bordersize: i32,
    is_checking_right_bound: bool,
) -> bool {
    // Gets the `gaps_out` value from Hyprland settings.
    let gaps_out = match Keyword::get("general:gaps_out") {
        Ok(value) => match value.value {
            hyprland::keyword::OptionValue::Int(v) => v as i32,
            hyprland::keyword::OptionValue::Float(v) => v as i32,
            _ => 0,
        },
        Err(_) => 0,
    };
    // Calculates the exact X-coordinates of the usable area's left and right edges.
    let mon_right = monitor.x + monitor.width as i32 - monitor.reserved.2 as i32 - gaps_out;
    let mon_left = monitor.x + monitor.reserved.3 as i32 + gaps_out;

    // Gets the X-coordinates of the active window.
    let act_right = (act.at.0 + act.size.0) as i32;
    let act_left = act.at.0 as i32;

    // Compares the window edge with the monitor edge, with a tolerance (`bordersize`).
    if is_checking_right_bound {
        (act_right - mon_right).abs() <= bordersize
    } else {
        (act_left - mon_left).abs() <= bordersize
    }
}

/// Finds the leftmost and rightmost clients on a given workspace.
/// Used to determine which window to focus when "jumping" from one workspace to another.
fn get_bound_client<'a>(
    all_clients: &'a Clients,
    workspace_id: i32,
    floating: bool,
) -> Option<(&'a Client, &'a Client)> {
    let ws_clients: Vec<&Client> = all_clients
        .iter()
        .filter(|c| {
            c.workspace.id == workspace_id
                && !c.workspace.name.starts_with("special")
                && c.floating == floating
        })
        .collect();

    if ws_clients.is_empty() {
        return None;
    }

    // Finds the client with the smallest X-coordinate (leftmost) and the largest (rightmost).
    let left_client = ws_clients.iter().min_by_key(|c| c.at.0)?;
    let right_client = ws_clients.iter().max_by_key(|c| c.at.0)?;
    Some((left_client, right_client))
}

