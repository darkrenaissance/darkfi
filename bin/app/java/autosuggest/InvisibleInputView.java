/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

package autosuggest;

import android.content.Context;
import android.graphics.Rect;
import android.text.Editable;
import android.text.Selection;
import android.util.Log;
import android.view.View;
import android.view.ViewGroup;
import android.view.inputmethod.BaseInputConnection;
import android.view.inputmethod.EditorInfo;
import android.view.inputmethod.InputConnection;

import autosuggest.CustomInputConnection;

public class InvisibleInputView extends View {
    public CustomInputConnection inputConnection;
    public int id = -1;
    public Editable editable;

    native static void onCreateInputConnect(int id);

    public InvisibleInputView(Context ctx, int id) {
        super(ctx);
        setFocusable(true);
        setFocusableInTouchMode(true);
        //setVisibility(INVISIBLE);
        setVisibility(VISIBLE);
        //setAlpha(0f);
        setLayoutParams(new ViewGroup.LayoutParams(400, 200));
        this.id = id;
        editable = Editable.Factory.getInstance().newEditable("");
        Selection.setSelection(editable, 0);
    }

    // Maybe move CustomInputConnection.setEditableText() to here?
    // For now this is called when the InputConnection is not yet available.
    public void setEditableText(String text) {
        editable.replace(0, editable.length(), text);
        Selection.setSelection(editable, text.length(), text.length());
    }
    // Same as above
    public void setSelection(int start, int end) {
        Selection.setSelection(editable, start, end);
    }

    @Override
    protected void onAttachedToWindow() {
        super.onAttachedToWindow();
        Log.d("darkfi", "InvisibleInputView " + id + " attached to window");
    }
    @Override
    public boolean onCheckIsTextEditor() {
        Log.d("darkfi", "onCheckIsTextEditor");
        return true;
    }

    @Override
    public InputConnection onCreateInputConnection(EditorInfo outAttrs) {
        Log.d("darkfi", "Create InputConnection for view=" + this.toString());
        if (inputConnection != null) {
            Log.d("darkfi", "  ->  return existing InputConnection");
            return inputConnection;
        }

        outAttrs.inputType = EditorInfo.TYPE_CLASS_TEXT
            | EditorInfo.TYPE_TEXT_FLAG_AUTO_CORRECT;
            //| EditorInfo.TYPE_TEXT_VARIATION_WEB_EDIT_TEXT;
        outAttrs.imeOptions = EditorInfo.IME_FLAG_NO_FULLSCREEN
            //| EditorInfo.IME_ACTION_NONE;
            | EditorInfo.IME_ACTION_GO;
        outAttrs.initialSelStart = getSelectionStart();
        outAttrs.initialSelEnd = getSelectionEnd();

        inputConnection = new CustomInputConnection(id, editable, this);
        onCreateInputConnect(id);
        return inputConnection;
    }

    @Override
    protected void onFocusChanged(boolean gainFocus, int direction, Rect previouslyFocusedRect) {
        super.onFocusChanged(gainFocus, direction, previouslyFocusedRect);
        Log.d("darkfi", "onFocusChanged: " + gainFocus);
    }

    public String debugEditableStr() {
        return CustomInputConnection.editableToXml(editable);
    }
    public String rawText() {
        return editable.toString();
    }
    public int getSelectionStart() {
        return Selection.getSelectionStart(editable);
    }
    public int getSelectionEnd() {
        return Selection.getSelectionEnd(editable);
    }
    public int getComposeStart() {
        return BaseInputConnection.getComposingSpanStart(editable);
    }
    public int getComposeEnd() {
        return BaseInputConnection.getComposingSpanEnd(editable);
    }
}

