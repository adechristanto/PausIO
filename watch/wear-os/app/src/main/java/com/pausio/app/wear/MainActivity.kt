package com.pausio.app.wear

import android.Manifest
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.focusable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.rotary.onRotaryScrollEvent
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.content.ContextCompat
import androidx.wear.compose.material3.Button
import androidx.wear.compose.material3.ButtonDefaults
import androidx.wear.compose.material3.MaterialTheme
import androidx.wear.compose.material3.Text
import kotlinx.coroutines.delay
import java.time.Duration
import java.time.Instant
import java.util.Locale

private val PausIOBlue = Color(0xFF2F8CFF)
private val PausIOBlueTrack = Color(0xFF09233D)
private val PausIOControlGray = Color(0xFF36383E)

private enum class WatchGlyph { Pause, Eye, Play, Check }

/** A compact, state-first control surface that mirrors the Apple Watch companion. */
class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        PausIOWearReminderScheduler.replace(this)
        setContent { PausIOWatchScreen() }
    }

    override fun onResume() {
        super.onResume()
        PausIOWearReminderScheduler.replace(this)
    }
}

@Composable
private fun PausIOWatchScreen() {
    val context = LocalContext.current
    var tick by remember { mutableIntStateOf(0) }
    val notificationPermission = rememberLauncherForActivityResult(
        ActivityResultContracts.RequestPermission(),
    ) { PausIOWearReminderScheduler.replace(context) }
    LaunchedEffect(Unit) {
        while (true) {
            delay(1_000)
            tick += 1
        }
    }

    val now = remember(tick) { Instant.now() }
    val state = remember(tick) { PausIOWearStateStore.read(context, now) }
    val locallyPaused = remember(tick) { PausIOWearStateStore.isLocallyPaused(context) }
    val remaining = state.phaseDeadlineAt
        ?.let { Duration.between(now, it).seconds.coerceAtLeast(0) }
    val visiblePhase = when {
        state.paused -> WatchTimerPhase.Paused
        state.phase in setOf(WatchTimerPhase.Working, WatchTimerPhase.PreBreak) && remaining == 0L -> WatchTimerPhase.BreakDue
        else -> state.phase
    }
    val scroll = rememberScrollState()

    MaterialTheme {
        Column(
            modifier = Modifier
                .fillMaxSize()
                .verticalScroll(scroll)
                .onRotaryScrollEvent {
                    scroll.dispatchRawDelta(it.verticalScrollPixels)
                    true
                }
                .focusable()
                .padding(horizontal = 16.dp, vertical = 6.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(7.dp, Alignment.CenterVertically),
        ) {
            PausIOWatchFace(
                remaining = remaining,
                phase = visiblePhase,
                remainingFraction = timerRemainingFraction(state, remaining),
            )
            Text(
                text = watchStatus(context, visiblePhase),
                color = Color.White.copy(alpha = 0.62f),
                fontSize = 11.sp,
                fontWeight = FontWeight.Medium,
                textAlign = TextAlign.Center,
                maxLines = 2,
            )
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
                ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
            ) {
                PausIOActionButton(
                    label = context.getString(R.string.wear_enable_alerts),
                    accessibilityLabel = context.getString(R.string.enable_notifications),
                ) {
                    notificationPermission.launch(Manifest.permission.POST_NOTIFICATIONS)
                }
            }
            PausIOContextualControls(
                context = context,
                state = state,
                phase = visiblePhase,
                locallyPaused = locallyPaused,
                onAction = { action ->
                    applyWatchAction(context, action)
                    tick += 1
                },
            )
        }
    }
}

@Composable
private fun PausIOContextualControls(
    context: android.content.Context,
    state: WatchTimerState,
    phase: WatchTimerPhase,
    locallyPaused: Boolean,
    onAction: (String) -> Unit,
) {
    when {
        phase == WatchTimerPhase.Dormant -> Unit
        phase == WatchTimerPhase.Breaking -> {
            PausIOIconControl(
                glyph = WatchGlyph.Check,
                label = context.getString(R.string.end_break),
                tint = Color(0xFF56D58A),
            ) { onAction("skip_break") }
        }
        phase == WatchTimerPhase.BreakDue -> {
            PausIOIconControl(
                glyph = WatchGlyph.Eye,
                label = context.getString(R.string.start_break),
                tint = PausIOBlue,
            ) { onAction("take_break_now") }
        }
        state.paused && locallyPaused -> {
            PausIOIconControl(
                glyph = WatchGlyph.Play,
                label = context.getString(R.string.resume_reminders),
                tint = Color(0xFF56D58A),
            ) { onAction("resume") }
        }
        state.paused -> Unit
        else -> {
            Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                PausIOIconControl(
                    glyph = WatchGlyph.Pause,
                    label = context.getString(R.string.pause_reminders),
                    tint = PausIOControlGray,
                ) { onAction("pause") }
                PausIOIconControl(
                    glyph = WatchGlyph.Eye,
                    label = context.getString(R.string.start_break),
                    tint = PausIOBlue,
                ) { onAction("take_break_now") }
            }
        }
    }
}

private fun applyWatchAction(context: android.content.Context, action: String) {
    when (action) {
        "skip_break" -> PausIOWearStateStore.finishLocalBreak(context)
        "resume" -> PausIOWearStateStore.setLocallyPaused(context, false)
        "pause" -> PausIOWearStateStore.setLocallyPaused(context, true)
        "take_break_now" -> PausIOWearStateStore.beginLocalBreak(context)
    }
    PausIOWearRuntimeAction.publish(context, action)
    PausIOWearReminderScheduler.replace(context)
}

@Composable
private fun PausIOWatchFace(
    remaining: Long?,
    phase: WatchTimerPhase,
    remainingFraction: Float,
) {
    val tint = if (phase == WatchTimerPhase.Breaking) Color(0xFF56D58A) else PausIOBlue
    val showsCountdown = phase in setOf(WatchTimerPhase.Working, WatchTimerPhase.PreBreak, WatchTimerPhase.Breaking)
    Box(modifier = Modifier.size(104.dp), contentAlignment = Alignment.Center) {
        Canvas(modifier = Modifier.fillMaxSize()) {
            val stroke = 7.dp.toPx()
            val radius = (size.minDimension - stroke) / 2
            drawCircle(
                color = tint.copy(alpha = 0.07f),
                radius = radius - stroke / 2,
            )
            drawCircle(
                color = if (phase == WatchTimerPhase.Paused) Color.White.copy(alpha = 0.16f) else PausIOBlueTrack,
                radius = radius,
                style = Stroke(width = stroke),
            )
            if (showsCountdown) {
                drawArc(
                    color = tint,
                    startAngle = -90f,
                    sweepAngle = 360f * remainingFraction.coerceIn(0f, 1f),
                    useCenter = false,
                    topLeft = Offset(center.x - radius, center.y - radius),
                    size = Size(radius * 2, radius * 2),
                    style = Stroke(width = stroke, cap = StrokeCap.Round),
                )
            }
        }
        when (phase) {
            WatchTimerPhase.Dormant -> Text("Zz", color = Color.White.copy(alpha = 0.68f), fontSize = 22.sp, fontWeight = FontWeight.Bold)
            WatchTimerPhase.Paused -> PausIOGlyph(WatchGlyph.Pause, Color.White.copy(alpha = 0.72f), Modifier.size(26.dp))
            WatchTimerPhase.BreakDue -> PausIOGlyph(WatchGlyph.Eye, PausIOBlue, Modifier.size(30.dp))
            else -> Text(
                text = remaining?.let(::formatDuration) ?: "--:--",
                color = Color.White,
                fontSize = 27.sp,
                fontWeight = FontWeight.Bold,
                textAlign = TextAlign.Center,
            )
        }
    }
}

@Composable
private fun PausIOActionButton(
    label: String,
    accessibilityLabel: String = label,
    enabled: Boolean = true,
    onClick: () -> Unit,
) {
    Button(
        onClick = onClick,
        enabled = enabled,
        modifier = Modifier
            .fillMaxWidth(0.66f)
            .heightIn(min = 38.dp)
            .semantics { contentDescription = accessibilityLabel },
        colors = ButtonDefaults.buttonColors(
            containerColor = PausIOBlue,
            contentColor = Color(0xFF10182D),
        ),
    ) {
        Text(
            text = label,
            fontSize = 13.sp,
            fontWeight = FontWeight.SemiBold,
            textAlign = TextAlign.Center,
            maxLines = 1,
        )
    }
}

@Composable
private fun PausIOIconControl(
    glyph: WatchGlyph,
    label: String,
    tint: Color,
    onClick: () -> Unit,
) {
    Button(
        onClick = onClick,
        modifier = Modifier.size(46.dp).semantics { contentDescription = label },
        colors = ButtonDefaults.buttonColors(
            containerColor = tint,
            contentColor = if (tint == PausIOControlGray) Color.White else Color(0xFF06111E),
        ),
    ) {
        PausIOGlyph(
            glyph = glyph,
            color = if (tint == PausIOControlGray) Color.White else Color(0xFF06111E),
            modifier = Modifier.size(19.dp),
        )
    }
}

@Composable
private fun PausIOGlyph(glyph: WatchGlyph, color: Color, modifier: Modifier = Modifier) {
    Canvas(modifier = modifier) {
        val stroke = 2.dp.toPx()
        when (glyph) {
            WatchGlyph.Pause -> {
                val barWidth = size.width * 0.2f
                val barHeight = size.height * 0.72f
                val top = (size.height - barHeight) / 2
                drawRoundRect(
                    color = color,
                    topLeft = Offset(size.width * 0.22f, top),
                    size = Size(barWidth, barHeight),
                    cornerRadius = CornerRadius(barWidth * 0.28f),
                )
                drawRoundRect(
                    color = color,
                    topLeft = Offset(size.width * 0.58f, top),
                    size = Size(barWidth, barHeight),
                    cornerRadius = CornerRadius(barWidth * 0.28f),
                )
            }
            WatchGlyph.Eye -> {
                val eye = Path().apply {
                    moveTo(size.width * 0.08f, size.height * 0.5f)
                    cubicTo(
                        size.width * 0.28f, size.height * 0.16f,
                        size.width * 0.72f, size.height * 0.16f,
                        size.width * 0.92f, size.height * 0.5f,
                    )
                    cubicTo(
                        size.width * 0.72f, size.height * 0.84f,
                        size.width * 0.28f, size.height * 0.84f,
                        size.width * 0.08f, size.height * 0.5f,
                    )
                    close()
                }
                drawPath(eye, color, style = Stroke(width = stroke, cap = StrokeCap.Round, join = StrokeJoin.Round))
                drawCircle(color, radius = size.minDimension * 0.13f)
            }
            WatchGlyph.Play -> {
                val play = Path().apply {
                    moveTo(size.width * 0.3f, size.height * 0.18f)
                    lineTo(size.width * 0.82f, size.height * 0.5f)
                    lineTo(size.width * 0.3f, size.height * 0.82f)
                    close()
                }
                drawPath(play, color)
            }
            WatchGlyph.Check -> {
                drawLine(
                    color,
                    Offset(size.width * 0.18f, size.height * 0.52f),
                    Offset(size.width * 0.42f, size.height * 0.76f),
                    strokeWidth = stroke,
                    cap = StrokeCap.Round,
                )
                drawLine(
                    color,
                    Offset(size.width * 0.42f, size.height * 0.76f),
                    Offset(size.width * 0.84f, size.height * 0.26f),
                    strokeWidth = stroke,
                    cap = StrokeCap.Round,
                )
            }
        }
    }
}

internal fun timerRemainingFraction(state: WatchTimerState, remaining: Long?): Float {
    val duration = when (state.phase) {
        WatchTimerPhase.Breaking -> state.breakDurationSeconds
        WatchTimerPhase.PreBreak -> state.preBreakSeconds
        WatchTimerPhase.Working -> state.workIntervalSeconds
        else -> 1L
    }.coerceAtLeast(1)
    return ((remaining ?: duration).toFloat() / duration.toFloat()).coerceIn(0f, 1f)
}

private fun phaseLabel(context: android.content.Context, phase: WatchTimerPhase): String = when (phase) {
    WatchTimerPhase.Dormant -> context.getString(R.string.phase_dormant)
    WatchTimerPhase.Working -> context.getString(R.string.phase_working)
    WatchTimerPhase.PreBreak -> context.getString(R.string.phase_pre_break)
    WatchTimerPhase.BreakDue -> context.getString(R.string.phase_break_due)
    WatchTimerPhase.Breaking -> context.getString(R.string.phase_breaking)
    WatchTimerPhase.Paused -> context.getString(R.string.phase_paused)
}

private fun watchStatus(context: android.content.Context, phase: WatchTimerPhase): String = when (phase) {
    WatchTimerPhase.Working, WatchTimerPhase.PreBreak -> context.getString(R.string.until_next_pause)
    WatchTimerPhase.Breaking -> context.getString(R.string.look_into_distance)
    else -> phaseLabel(context, phase)
}

private fun formatDuration(seconds: Long): String = String.format(
    Locale.getDefault(), "%02d:%02d", seconds / 60, seconds % 60,
)
