package com.argentum.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.argentum.ui.theme.GlassHighlight
import com.argentum.ui.theme.GlassSilver
import com.argentum.ui.theme.Silver

@Composable
fun GlassCard(
    modifier: Modifier = Modifier,
    cornerRadius: Dp = 16.dp,
    backgroundColor: Color = MaterialTheme.colorScheme.surface,
    borderColor: Color = Silver.copy(alpha = 0.3f),
    content: @Composable () -> Unit
) {
    val shape = RoundedCornerShape(cornerRadius)
    
    Box(
        modifier = modifier
            .clip(shape)
            .background(
                brush = Brush.verticalGradient(
                    colors = listOf(
                        backgroundColor.copy(alpha = 0.9f),
                        backgroundColor.copy(alpha = 0.7f)
                    )
                )
            )
            .border(
                width = 1.dp,
                color = borderColor,
                shape = shape
            )
            .padding(1.dp)
    ) {
        content()
    }
}

@Composable
fun GlassSurface(
    modifier: Modifier = Modifier,
    content: @Composable () -> Unit
) {
    Box(
        modifier = modifier
            .background(
                brush = Brush.verticalGradient(
                    colors = listOf(
                        GlassSilver,
                        GlassHighlight
                    )
                )
            )
    ) {
        content()
    }
}

@Composable
fun GlassButton(
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    onClick: () -> Unit,
    content: @Composable () -> Unit
) {
    val shape = RoundedCornerShape(24.dp)
    
    Box(
        modifier = modifier
            .clip(shape)
            .background(
                brush = Brush.horizontalGradient(
                    colors = if (enabled) {
                        listOf(
                            Silver.copy(alpha = 0.3f),
                            Silver.copy(alpha = 0.5f)
                        )
                    } else {
                        listOf(
                            Color.Gray.copy(alpha = 0.2f),
                            Color.Gray.copy(alpha = 0.3f)
                        )
                    }
                )
            )
            .border(
                width = 1.dp,
                color = if (enabled) Silver.copy(alpha = 0.5f) else Color.Gray.copy(alpha = 0.3f),
                shape = shape
            )
    ) {
        content()
    }
}